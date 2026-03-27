use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{error, info, warn};

use crate::config::NetworkConfig;
use crate::database::ConnectionPool;
use crate::enhanced_server::EnhancedHttpServer;
use crate::error::NetworkError;
use crate::error_middleware::{ErrorMiddleware, ErrorMiddlewareConfig};
use crate::metrics::MetricsCollector;

pub mod config;
pub mod database;
pub mod enhanced_server;
pub mod error;
pub mod error_middleware;
pub mod metrics;
pub mod p2p;
pub mod server;
pub mod shutdown;
pub mod state_trie;

/// Main network node application
pub struct NetworkNode {
    config: NetworkConfig,
    connection_pool: Arc<RwLock<ConnectionPool>>,
    http_server: EnhancedHttpServer,
    shutdown_handler: shutdown::ShutdownHandler,
    error_middleware: Arc<ErrorMiddleware>,
    metrics_collector: Arc<MetricsCollector>,
    state_trie: Arc<RwLock<state_trie::StateTrie>>,
    p2p_manager: Arc<p2p::P2PManager>,
}

impl NetworkNode {
    /// Create a new network node instance
    pub async fn new(config: NetworkConfig) -> Result<Self, NetworkError> {
        info!("Initializing network node with config: {:?}", config);

        // Initialize error middleware
        let error_middleware = Arc::new(ErrorMiddleware::new(ErrorMiddlewareConfig::default()));

        // Initialize metrics collector
        let metrics_collector = Arc::new(MetricsCollector::new());

        // Initialize database connection pool
        let connection_pool = Arc::new(RwLock::new(
            ConnectionPool::new(&config.database_url).await?,
        ));

        // Initialize state trie
        let state_trie = Arc::new(RwLock::new(state_trie::StateTrie::new(
            "./data/state_trie",
        )?));

        // Initialize P2P manager
        let local_id = [0u8; 32]; // Replace with actual node ID generation
        let p2p_manager = Arc::new(p2p::P2PManager::new(local_id));

        // Initialize enhanced HTTP server
        let http_server = EnhancedHttpServer::new(
            config.clone(),
            connection_pool.clone(),
            error_middleware.clone(),
            metrics_collector.clone(),
            state_trie.clone(),
            p2p_manager.clone(),
        );

        // Initialize shutdown handler
        let shutdown_handler = shutdown::ShutdownHandler::new(config.shutdown_grace_period);

        Ok(Self {
            config,
            connection_pool,
            http_server,
            shutdown_handler,
            error_middleware,
            metrics_collector,
            state_trie,
            p2p_manager,
        })
    }

    /// Start the network node
    pub async fn start(mut self) -> Result<(), NetworkError> {
        info!("Starting network node...");

        // Start shutdown handler in background
        let shutdown_signal = self.shutdown_handler.start();

        // Start HTTP server
        let server_handle = self.http_server.start().await?;

        // Start P2P maintenance worker
        self.p2p_manager.start_maintenance().await;

        // Bootstrap if peer exists
        if let Some(seed) = self.config.bootstrap_peer.clone() {
            let seed_addr: std::net::SocketAddr = seed
                .parse()
                .map_err(|e| NetworkError::Config(format!("Invalid seed address: {}", e)))?;
            self.p2p_manager.bootstrap(seed_addr).await?;
        }

        info!("Network node started successfully");

        // Wait for shutdown signal
        tokio::select! {
            result = server_handle => {
                match result {
                    Ok(_) => info!("HTTP server stopped gracefully"),
                    Err(e) => error!("HTTP server error: {:?}", e),
                }
            }
            _ = shutdown_signal => {
                info!("Shutdown signal received, initiating graceful shutdown");
                self.shutdown().await?;
            }
        }

        Ok(())
    }

    /// Perform graceful shutdown
    async fn shutdown(&mut self) -> Result<(), NetworkError> {
        info!("Starting graceful shutdown sequence...");

        // Step 1: Stop accepting new connections immediately
        info!("Stopping acceptance of new connections...");
        self.http_server.stop_accepting_new_connections().await?;

        // Step 2: Wait for active operations to finish (grace period)
        let grace_period = self.config.shutdown_grace_period;
        info!(
            "Waiting for active operations to finish ({} seconds)...",
            grace_period.as_secs()
        );

        let shutdown_result = timeout(grace_period, async {
            // Wait for all active HTTP connections to complete
            self.http_server.wait_for_connections_to_complete().await?;

            // Wait for any database operations to complete
            self.wait_for_database_operations().await?;

            Ok::<(), NetworkError>(())
        })
        .await;

        match shutdown_result {
            Ok(Ok(())) => {
                info!("All operations completed gracefully");
            }
            Ok(Err(e)) => {
                warn!("Error during graceful shutdown: {:?}", e);
            }
            Err(_) => {
                warn!("Grace period expired, forcing shutdown");
            }
        }

        // Step 3: Close database connection pools
        info!("Closing database connection pools...");
        self.close_database_connections().await?;

        // Step 4: Stop the HTTP server completely
        info!("Stopping HTTP server...");
        self.http_server.stop().await?;

        info!("Graceful shutdown completed");
        Ok(())
    }

    /// Wait for database operations to complete
    async fn wait_for_database_operations(&self) -> Result<(), NetworkError> {
        let pool = self.connection_pool.read().await;

        // Wait for all active connections to become idle
        let mut attempts = 0;
        let max_attempts = 30; // 30 seconds with 1-second intervals

        while attempts < max_attempts {
            let active_connections = pool.active_connections();
            if active_connections == 0 {
                info!("All database connections are idle");
                break;
            }

            if attempts % 5 == 0 {
                info!(
                    "Waiting for {} active database connections to complete...",
                    active_connections
                );
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
            attempts += 1;
        }

        if attempts >= max_attempts {
            warn!("Database operations did not complete within grace period");
        }

        Ok(())
    }

    /// Close all database connections
    async fn close_database_connections(&mut self) -> Result<(), NetworkError> {
        let mut pool = self.connection_pool.write().await;
        pool.close_all().await?;
        info!("All database connections closed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DatabaseConfig;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_graceful_shutdown() {
        let config = NetworkConfig {
            bind_address: "127.0.0.1:0".to_string(),
            database_url: "sqlite::memory:".to_string(),
            database_config: DatabaseConfig::default(),
            shutdown_grace_period: Duration::from_secs(5),
            log_level: "info".to_string(),
            bootstrap_peer: None,
        };

        let node = NetworkNode::new(config).await.unwrap();

        // Simulate shutdown signal
        let node_clone = node.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            // This would normally be triggered by OS signal
        });

        // Test should complete within grace period
        let result = node.start().await;
        assert!(result.is_ok());
    }
}
