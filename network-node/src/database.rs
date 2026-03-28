use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::timeout;
use tracing::{debug, error, info, warn, instrument};

use crate::config::DatabaseConfig;
use crate::error::{DatabaseError, NetworkError, Result};

/// Database connection pool manager
pub struct ConnectionPool {
    connections: Arc<RwLock<Vec<DatabaseConnection>>>,
    config: DatabaseConfig,
    max_connections: usize,
    active_connections: Arc<RwLock<usize>>,
}

/// Individual database connection
#[derive(Debug)]
struct DatabaseConnection {
    id: String,
    created_at: chrono::DateTime<chrono::Utc>,
    last_used: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    is_active: Arc<RwLock<bool>>,
}

impl ConnectionPool {
    /// Create a new connection pool
    #[instrument(fields(database_url = %database_url, max_connections = %config.max_connections))]
    pub async fn new(database_url: &str) -> Result<Self> {
        let config = DatabaseConfig::from_url(database_url)?;

        info!(
            "Creating database connection pool with max {} connections",
            config.max_connections
        );

        let pool = Self {
            connections: Arc::new(RwLock::new(Vec::new())),
            config,
            max_connections: config.max_connections,
            active_connections: Arc::new(RwLock::new(0)),
        };

        // Initialize minimum connections
        pool.initialize_min_connections().await?;

        Ok(pool)
    }

    /// Initialize minimum number of connections
    #[instrument(skip(self), fields(min_connections = %self.config.min_connections))]
    async fn initialize_min_connections(&self) -> Result<()> {
        let mut connections = self.connections.write().await;

        for i in 0..self.config.min_connections {
            let conn = self.create_connection(i).await?;
            connections.push(conn);
        }

        info!(
            "Initialized {} database connections",
            self.config.min_connections
        );
        Ok(())
    }

    /// Create a new database connection
    #[instrument(skip(self), fields(connection_id, pool_size = %self.connections.read().await.len()))]
    async fn create_connection(&self, id: usize) -> Result<DatabaseConnection> {
        let connection_id = format!("conn_{}_{}", id, uuid::Uuid::new_v4());
        tracing::Span::current().record("connection_id", &tracing::field::display(&connection_id));

        // In a real implementation, this would establish an actual database connection
        // For now, we simulate connection creation
        info!("Creating database connection: {}", connection_id);

        let conn = DatabaseConnection {
            id: connection_id,
            created_at: chrono::Utc::now(),
            last_used: Arc::new(RwLock::new(chrono::Utc::now())),
            is_active: Arc::new(RwLock::new(true)),
        };

        Ok(conn)
    }

    /// Get a connection from the pool
    #[instrument(skip(self), fields(active_connections = %self.active_connections.read().await, total_connections = %self.connections.read().await.len()))]
    pub async fn get_connection(&self) -> Result<ConnectionHandle> {
        let mut connections = self.connections.write().await;

        // Try to find an available connection
        for conn in connections.iter() {
            let mut is_active = conn.is_active.write().await;
            if !*is_active {
                *is_active = true;
                *conn.last_used.write().await = chrono::Utc::now();

                // Increment active connections counter
                *self.active_connections.write().await += 1;

                debug!("Reusing existing connection: {}", conn.id);
                return Ok(ConnectionHandle {
                    connection_id: conn.id.clone(),
                    pool: self.clone(),
                });
            }
        }

        // If no available connection and we can create more
        if connections.len() < self.max_connections {
            let new_conn = self.create_connection(connections.len()).await?;
            let connection_id = new_conn.id.clone();
            connections.push(new_conn);

            // Increment active connections counter
            *self.active_connections.write().await += 1;

            debug!("Created new connection: {}", connection_id);
            return Ok(ConnectionHandle {
                connection_id,
                pool: self.clone(),
            });
        }

        // Pool exhausted
        error!("Database connection pool exhausted");
        Err(NetworkError::Database(DatabaseError::PoolExhausted))
    }

    /// Return a connection to the pool
    async fn return_connection(&self, connection_id: &str) {
        let connections = self.connections.read().await;

        if let Some(conn) = connections.iter().find(|c| c.id == connection_id) {
            *conn.is_active.write().await = false;
            *conn.last_used.write().await = chrono::Utc::now();
        }

        // Decrement active connections counter
        let mut active_count = self.active_connections.write().await;
        if *active_count > 0 {
            *active_count -= 1;
        }
    }

    /// Get number of active connections
    pub async fn active_connections(&self) -> usize {
        *self.active_connections.read().await
    }

    /// Get total number of connections in pool
    pub async fn total_connections(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Close all connections
    pub async fn close_all(&mut self) -> Result<()> {
        info!("Closing all database connections...");

        let connections = self.connections.read().await;

        // Wait for all connections to become inactive
        let mut attempts = 0;
        let max_attempts = 30;

        while attempts < max_attempts {
            let active_count = self.active_connections().await;
            if active_count == 0 {
                break;
            }

            if attempts % 5 == 0 {
                info!(
                    "Waiting for {} active connections to close...",
                    active_count
                );
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
            attempts += 1;
        }

        if attempts >= max_attempts {
            warn!("Some connections did not close gracefully");
        }

        // In a real implementation, this would close actual database connections
        info!("All database connections closed");
        Ok(())
    }

    /// Perform health check on all connections
    pub async fn health_check(&self) -> Result<bool> {
        let connections = self.connections.read().await;

        for conn in connections.iter() {
            // In a real implementation, this would perform an actual health check
            // For now, we just check if the connection is marked as active
            let is_active = *conn.is_active.read().await;
            if is_active {
                let last_used = *conn.last_used.read().await;
                let age = chrono::Utc::now() - last_used;

                // If connection has been active for too long, it might be stuck
                if age > chrono::Duration::minutes(5) {
                    warn!("Connection {} appears to be stuck", conn.id);
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
}

impl Clone for ConnectionPool {
    fn clone(&self) -> Self {
        Self {
            connections: self.connections.clone(),
            config: self.config.clone(),
            max_connections: self.max_connections,
            active_connections: self.active_connections.clone(),
        }
    }
}

/// Handle for a database connection
pub struct ConnectionHandle {
    connection_id: String,
    pool: ConnectionPool,
}

impl ConnectionHandle {
    /// Execute a query (simulated)
    #[instrument(skip(self), fields(connection_id = %self.connection_id, query_hash = %format!("{:x}", sha2::Sha256::digest(query.as_bytes()))))]
    pub async fn execute_query(&self, query: &str) -> Result<String> {
        let start_time = std::time::Instant::now();
        
        info!(
            "Executing query on connection {}: {}",
            self.connection_id, query
        );

        // In a real implementation, this would execute an actual database query
        // For now, we simulate query execution
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        let duration = start_time.elapsed();
        debug!("Query executed in {:?} on connection {}", duration, self.connection_id);

        Ok(format!(
            "Query executed on connection {}",
            self.connection_id
        ))
    }

    /// Get connection ID
    pub fn id(&self) -> &str {
        &self.connection_id
    }
}

impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        let connection_id = self.connection_id.clone();
        let pool = self.pool.clone();

        // Return connection to pool asynchronously
        tokio::spawn(async move {
            pool.return_connection(&connection_id).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_connection_pool_basic() {
        let pool = ConnectionPool::new("sqlite::memory:").await.unwrap();

        assert_eq!(pool.active_connections().await, 0);
        assert_eq!(pool.total_connections().await, 2); // min_connections

        // Get a connection
        let conn = pool.get_connection().await.unwrap();
        assert_eq!(pool.active_connections().await, 1);

        // Use the connection
        let result = conn.execute_query("SELECT 1").await.unwrap();
        assert!(result.contains("Query executed"));

        // Connection should be returned when dropped
        drop(conn);
        sleep(Duration::from_millis(100)).await;
        assert_eq!(pool.active_connections().await, 0);
    }

    #[tokio::test]
    async fn test_connection_pool_exhaustion() {
        let config = DatabaseConfig {
            min_connections: 1,
            max_connections: 2,
            connection_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300),
        };

        let pool = ConnectionPool {
            connections: Arc::new(RwLock::new(Vec::new())),
            config,
            max_connections: 2,
            active_connections: Arc::new(RwLock::new(0)),
        };

        // Get all connections
        let conn1 = pool.get_connection().await.unwrap();
        let conn2 = pool.get_connection().await.unwrap();

        // Pool should be exhausted now
        let conn3_result = pool.get_connection().await;
        assert!(conn3_result.is_err());
        assert!(matches!(
            conn3_result.unwrap_err(),
            NetworkError::Database(DatabaseError::PoolExhausted)
        ));

        // Return one connection
        drop(conn1);
        sleep(Duration::from_millis(100)).await;

        // Should be able to get a connection again
        let conn3 = pool.get_connection().await.unwrap();
        assert!(conn3.id() != conn2.id());
    }
}
