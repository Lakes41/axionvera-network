use std::time::SystemTime;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status, Streaming};
use tokio_stream::wrappers::ReceiverStream;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::grpc::network::{
    health_service_server::HealthService,
    HealthCheckResponse, HealthStatus,
};
use crate::database::ConnectionPool;

pub struct HealthServiceImpl {
    connection_pool: Arc<RwLock<ConnectionPool>>,
}

impl HealthServiceImpl {
    pub fn new(connection_pool: Arc<RwLock<ConnectionPool>>) -> Self {
        Self {
            connection_pool,
        }
    }

    async fn check_database_health(&self) -> (bool, String) {
        let pool = self.connection_pool.read().await;
        match pool.health_check().await {
            Ok(_) => (true, "Database connection healthy".to_string()),
            Err(e) => (false, format!("Database connection failed: {}", e)),
        }
    }

    async fn check_overall_health(&self) -> (HealthStatus, String, std::collections::HashMap<String, String>) {
        let mut details = std::collections::HashMap::new();
        let mut overall_status = HealthStatus::Serving;

        // Check database health
        let (db_healthy, db_message) = self.check_database_health().await;
        details.insert("database".to_string(), if db_healthy { "healthy".to_string() } else { "unhealthy".to_string() });
        
        if !db_healthy {
            overall_status = HealthStatus::NotServing;
        }

        // Check memory usage (mock implementation)
        let memory_usage = "45%".to_string();
        details.insert("memory_usage".to_string(), memory_usage.clone());
        
        // Check CPU usage (mock implementation)
        let cpu_usage = "23%".to_string();
        details.insert("cpu_usage".to_string(), cpu_usage.clone());

        let message = if overall_status == HealthStatus::Serving {
            "All systems operational".to_string()
        } else {
            "Some systems are unhealthy".to_string()
        };

        (overall_status, message, details)
    }
}

#[tonic::async_trait]
impl HealthService for HealthServiceImpl {
    async fn check(&self, _request: Request<()>) -> Result<Response<HealthCheckResponse>, Status> {
        info!("Health check requested");

        let (status, message, details) = self.check_overall_health().await;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

        let response = HealthCheckResponse {
            status: status as i32,
            message,
            timestamp: Some(prost_types::Timestamp {
                seconds: timestamp.as_secs() as i64,
                nanos: timestamp.subsec_nanos() as i32,
            }),
            details,
        };

        Ok(Response::new(response))
    }

    type WatchStream = ReceiverStream<Result<HealthCheckResponse, Status>>;

    async fn watch(&self, _request: Request<()>) -> Result<Response<Self::WatchStream>, Status> {
        info!("Health watch requested");

        let (tx, rx) = mpsc::channel(100);
        let connection_pool = self.connection_pool.clone();

        // Spawn a task to periodically send health updates
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            
            loop {
                interval.tick().await;
                
                let (status, message, details) = {
                    let mut details = std::collections::HashMap::new();
                    let mut overall_status = HealthStatus::Serving;

                    // Check database health
                    let pool = connection_pool.read().await;
                    let (db_healthy, db_message) = pool.health_check().await.unwrap_or_else(|e| {
                        (false, format!("Database connection failed: {}", e))
                    });
                    drop(pool);
                    
                    details.insert("database".to_string(), if db_healthy { "healthy".to_string() } else { "unhealthy".to_string() });
                    
                    if !db_healthy {
                        overall_status = HealthStatus::NotServing;
                    }

                    // Add mock metrics
                    details.insert("memory_usage".to_string(), "45%".to_string());
                    details.insert("cpu_usage".to_string(), "23%".to_string());

                    let message = if overall_status == HealthStatus::Serving {
                        "All systems operational".to_string()
                    } else {
                        "Some systems are unhealthy".to_string()
                    };

                    (overall_status, message, details)
                };

                let timestamp = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default();

                let response = HealthCheckResponse {
                    status: status as i32,
                    message,
                    timestamp: Some(prost_types::Timestamp {
                        seconds: timestamp.as_secs() as i64,
                        nanos: timestamp.subsec_nanos() as i32,
                    }),
                    details,
                };

                if tx.send(Ok(response)).await.is_err() {
                    warn!("Health watch client disconnected");
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
