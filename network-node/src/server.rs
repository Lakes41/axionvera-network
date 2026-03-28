use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{error, info, warn};

use crate::config::NetworkConfig;
use crate::database::ConnectionPool;
use crate::error::{NetworkError, Result};

/// HTTP server for the network node
pub struct HttpServer {
    config: NetworkConfig,
    connection_pool: Arc<RwLock<ConnectionPool>>,
    is_accepting_connections: Arc<RwLock<bool>>,
    active_connections: Arc<RwLock<usize>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl HttpServer {
    /// Create a new HTTP server
    pub fn new(config: NetworkConfig, connection_pool: Arc<RwLock<ConnectionPool>>) -> Self {
        Self {
            config,
            connection_pool,
            is_accepting_connections: Arc::new(RwLock::new(true)),
            active_connections: Arc::new(RwLock::new(0)),
            shutdown_tx: None,
        }
    }

    /// Start the HTTP server
    pub async fn start(&mut self) -> Result<tokio::task::JoinHandle<Result<()>>> {
        info!("Starting HTTP server on {}", self.config.bind_address);

        let bind_addr = self.config.bind_address.clone();
        let is_accepting = self.is_accepting_connections.clone();
        let active_connections = self.active_connections.clone();
        let connection_pool = self.connection_pool.clone();

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        // In a real implementation, this would start an actual HTTP server (e.g., using hyper or axum)
        // For now, we simulate the server behavior
        let handle = tokio::spawn(async move {
            Self::run_server_simulation(
                bind_addr,
                is_accepting,
                active_connections,
                connection_pool,
                shutdown_rx,
            )
            .await
        });

        Ok(handle)
    }

    /// Stop accepting new connections
    pub async fn stop_accepting_new_connections(&self) -> Result<()> {
        info!("Stopping acceptance of new HTTP connections");
        *self.is_accepting_connections.write().await = false;
        Ok(())
    }

    /// Wait for all active connections to complete
    pub async fn wait_for_connections_to_complete(&self) -> Result<()> {
        info!("Waiting for active HTTP connections to complete");

        let mut attempts = 0;
        let max_attempts = 60; // 60 seconds with 1-second intervals

        while attempts < max_attempts {
            let active_count = *self.active_connections.read().await;
            if active_count == 0 {
                info!("All HTTP connections have completed");
                break;
            }

            if attempts % 10 == 0 {
                info!("Waiting for {} active HTTP connections...", active_count);
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
            attempts += 1;
        }

        if attempts >= max_attempts {
            warn!("HTTP connections did not complete within timeout period");
        }

        Ok(())
    }

    /// Stop the HTTP server completely
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping HTTP server");

        // Send shutdown signal
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        Ok(())
    }

    /// Simulated server implementation
    async fn run_server_simulation(
        bind_addr: String,
        is_accepting: Arc<RwLock<bool>>,
        active_connections: Arc<RwLock<usize>>,
        connection_pool: Arc<RwLock<ConnectionPool>>,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<()> {
        info!("Server simulation running on {}", bind_addr);

        // Simulate handling incoming connections
        let mut connection_counter = 0;

        loop {
            // Check if we should accept new connections
            let accepting = *is_accepting.read().await;

            if !accepting {
                info!("Server no longer accepting new connections");
                break;
            }

            // Simulate connection handling
            tokio::select! {
                _ = shutdown_rx => {
                    info!("Server shutdown signal received");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Simulate occasional connections
                    if connection_counter % 10 == 0 {
                        Self::handle_simulated_connection(
                            connection_counter,
                            active_connections.clone(),
                            connection_pool.clone(),
                        ).await;
                    }
                    connection_counter += 1;
                }
            }
        }

        info!("Server simulation stopped");
        Ok(())
    }

    /// Simulate handling a single connection
    async fn handle_simulated_connection(
        conn_id: usize,
        active_connections: Arc<RwLock<usize>>,
        connection_pool: Arc<RwLock<ConnectionPool>>,
    ) {
        // Increment active connections
        *active_connections.write().await += 1;

        info!("Handling simulated connection {}", conn_id);

        // Simulate connection work
        let work_duration = Duration::from_millis(50 + (conn_id % 200) as u64);
        tokio::time::sleep(work_duration).await;

        // Simulate database operation
        if let Ok(pool) = connection_pool.try_read() {
            if let Ok(conn) = pool.get_connection().await {
                let _ = conn.execute_query("SELECT * FROM test").await;
                // Connection is automatically returned when dropped
            }
        }

        // Decrement active connections
        *active_connections.write().await -= 1;

        info!("Completed simulated connection {}", conn_id);
    }

    /// Get server statistics
    pub async fn get_stats(&self) -> ServerStats {
        ServerStats {
            is_accepting_connections: *self.is_accepting_connections.read().await,
            active_connections: *self.active_connections.read().await,
            bind_address: self.config.bind_address.clone(),
        }
    }
}

/// Server statistics
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub is_accepting_connections: bool,
    pub active_connections: usize,
    pub bind_address: String,
}

/// Health check endpoint handler
pub async fn health_check() -> HealthStatus {
    // In a real implementation, this would check various system components
    HealthStatus {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime: chrono::Utc::now() - chrono::Utc::now(), // This would be actual uptime
    }
}

/// Ready check endpoint handler
pub async fn ready_check(connection_pool: Arc<RwLock<ConnectionPool>>) -> ReadyStatus {
    let pool = connection_pool.read().await;

    let database_healthy = pool.health_check().await.unwrap_or(false);
    let active_connections = pool.active_connections().await;

    ReadyStatus {
        ready: database_healthy && active_connections > 0,
        database_healthy,
        active_connections,
        timestamp: chrono::Utc::now(),
    }
}

/// Health status response
#[derive(Debug, serde::Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub version: String,
    pub uptime: chrono::Duration,
}

/// Ready status response
#[derive(Debug, serde::Serialize)]
pub struct ReadyStatus {
    pub ready: bool,
    pub database_healthy: bool,
    pub active_connections: usize,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DatabaseConfig;

    #[tokio::test]
    async fn test_server_lifecycle() {
        let config = NetworkConfig {
            bind_address: "127.0.0.1:0".to_string(),
            database_url: "sqlite::memory:".to_string(),
            database_config: DatabaseConfig::default(),
            shutdown_grace_period: Duration::from_secs(5),
            log_level: "info".to_string(),
        };

        let connection_pool = Arc::new(RwLock::new(
            ConnectionPool::new("sqlite::memory:").await.unwrap(),
        ));

        let mut server = HttpServer::new(config.clone(), connection_pool.clone());

        // Start server
        let handle = server.start().await.unwrap();

        // Check initial stats
        let stats = server.get_stats().await;
        assert!(stats.is_accepting_connections);
        assert_eq!(stats.active_connections, 0);

        // Stop accepting new connections
        server.stop_accepting_new_connections().await.unwrap();

        let stats = server.get_stats().await;
        assert!(!stats.is_accepting_connections);

        // Wait for server to finish
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_health_endpoints() {
        let health = health_check().await;
        assert_eq!(health.status, "healthy");

        let connection_pool = Arc::new(RwLock::new(
            ConnectionPool::new("sqlite::memory:").await.unwrap(),
        ));

        let ready = ready_check(connection_pool).await;
        assert!(ready.ready);
        assert!(ready.database_healthy);
    }
}
