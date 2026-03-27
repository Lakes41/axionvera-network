use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub bind_address: String,
    pub database_url: String,
    pub database_config: DatabaseConfig,
    pub shutdown_grace_period: Duration,
    pub log_level: String,
    pub bootstrap_peer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub min_connections: usize,
    pub max_connections: usize,
    pub connection_timeout: Duration,
    pub idle_timeout: Duration,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            min_connections: 2,
            max_connections: 10,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
        }
    }
}

impl DatabaseConfig {
    pub fn from_url(url: &str) -> crate::error::Result<Self> {
        // In a real implementation, this would parse the database URL
        // and extract configuration parameters
        Ok(Self::default())
    }
}

impl NetworkConfig {
    pub fn from_env() -> crate::error::Result<Self> {
        // Load configuration from environment variables
        let bind_address =
            std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());

        let shutdown_grace_period_secs = std::env::var("SHUTDOWN_GRACE_PERIOD")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<u64>()
            .map_err(|_| {
                crate::error::NetworkError::Config("Invalid SHUTDOWN_GRACE_PERIOD".to_string())
            })?;

        let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

        Ok(Self {
            bind_address,
            database_url,
            database_config: DatabaseConfig::default(),
            shutdown_grace_period: Duration::from_secs(shutdown_grace_period_secs),
            log_level,
            bootstrap_peer: std::env::var("BOOTSTRAP_PEER").ok(),
        })
    }
}
