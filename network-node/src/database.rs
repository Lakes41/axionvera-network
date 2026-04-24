use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, instrument};
use sqlx::postgres::{PgPoolOptions, PgPool};
use crate::config::DatabaseConfig;
use crate::error::{DatabaseError, Result};

/// Database connection pool manager using sqlx
#[derive(Clone)]
pub struct ConnectionPool {
    pool: PgPool,
    config: DatabaseConfig,
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

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections as u32)
            .min_connections(config.min_connections as u32)
            .acquire_timeout(config.connection_timeout)
            .idle_timeout(config.idle_timeout)
            .connect(database_url)
            .await
            .map_err(|e| crate::error::NetworkError::Database(DatabaseError::ConnectionFailed(e.to_string())))?;

        let pool_manager = Self {
            pool,
            config,
        };

        // Initialize schema
        pool_manager.initialize_schema().await?;

        Ok(pool_manager)
    }

    /// Initialize database schema
    async fn initialize_schema(&self) -> Result<()> {
        info!("Initializing database schema...");

        // Create indexer_state table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS indexer_state (
                id SERIAL PRIMARY KEY,
                last_processed_ledger INTEGER NOT NULL DEFAULT 0,
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
            )"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| crate::error::NetworkError::Database(DatabaseError::QueryFailed(e.to_string())))?;

        // Initialize indexer_state if empty
        sqlx::query(
            "INSERT INTO indexer_state (id, last_processed_ledger)
             SELECT 1, 0
             WHERE NOT EXISTS (SELECT 1 FROM indexer_state WHERE id = 1)"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| crate::error::NetworkError::Database(DatabaseError::QueryFailed(e.to_string())))?;

        // Create transactions table with composite unique constraint
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS transactions (
                id SERIAL PRIMARY KEY,
                transaction_hash TEXT NOT NULL,
                event_index INTEGER NOT NULL,
                ledger_sequence INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                data JSONB NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(transaction_hash, event_index)
            )"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| crate::error::NetworkError::Database(DatabaseError::QueryFailed(e.to_string())))?;

        info!("Database schema initialized successfully");
        Ok(())
    }

    /// Get the underlying sqlx pool
    pub fn get_pool(&self) -> &PgPool {
        &self.pool
    }

    /// Close the connection pool
    pub async fn close_all(&mut self) -> Result<()> {
        info!("Closing database connection pool...");
        self.pool.close().await;
        info!("Database connection pool closed");
        Ok(())
    }

    /// Perform health check
    pub async fn health_check(&self) -> Result<bool> {
        match sqlx::query("SELECT 1").execute(&self.pool).await {
            Ok(_) => Ok(true),
            Err(e) => {
                error!("Database health check failed: {}", e);
                Ok(false)
            }
        }
    }
}
