use serde::{Deserialize, Serialize};
use crate::signing::SignerConfig;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TracingExporter {
    Otlp,
    Jaeger,
    XRay,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub bind_address: String,
    pub grpc_bind_address: String,
    pub gateway_bind_address: String,
    pub database_url: String,
    pub database_config: DatabaseConfig,
    pub shutdown_grace_period: Duration,
    pub log_level: String,
    pub bootstrap_peer: Option<String>,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub enable_gateway: bool,
    pub enable_reflection: bool,
    pub node_id: String,
    pub otlp_endpoint: Option<String>,
    pub jaeger_endpoint: Option<String>,
    pub xray_endpoint: Option<String>,
    pub tracing_enabled: bool,
    pub tracing_exporter: TracingExporter,
    pub signing_config: Option<SignerConfig>,
    pub cache_ttl_seconds: i64,
    /// Path to genesis JSON (`config/genesis.example.json`). See `GENESIS_CONFIG_PATH`.
    pub genesis_config_path: Option<String>,
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

        let node_id = std::env::var("NODE_ID").unwrap_or_else(|_| {
            format!("node-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("unknown"))
        });

        let tracing_enabled = std::env::var("TRACING_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        let tracing_exporter_str = std::env::var("TRACING_EXPORTER").unwrap_or_else(|_| "otlp".to_string());
        let tracing_exporter = match tracing_exporter_str.to_lowercase().as_str() {
            "jaeger" => TracingExporter::Jaeger,
            "xray" => TracingExporter::XRay,
            "none" => TracingExporter::None,
            _ => TracingExporter::Otlp,
        };

        let cache_ttl_seconds = std::env::var("CACHE_TTL_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3600);

        let genesis_config_path = std::env::var("GENESIS_CONFIG_PATH").ok();

        Ok(Self {
            bind_address,
            grpc_bind_address: std::env::var("GRPC_BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:50051".to_string()),
            gateway_bind_address: std::env::var("GATEWAY_BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:8081".to_string()),
            database_url,
            database_config: DatabaseConfig::default(),
            shutdown_grace_period: Duration::from_secs(shutdown_grace_period_secs),
            log_level,
            bootstrap_peer: std::env::var("BOOTSTRAP_PEER").ok(),
            tls_cert_path: std::env::var("TLS_CERT_PATH").ok(),
            tls_key_path: std::env::var("TLS_KEY_PATH").ok(),
            enable_gateway: std::env::var("ENABLE_GATEWAY").unwrap_or_else(|_| "true".to_string()).parse().unwrap_or(true),
            enable_reflection: std::env::var("ENABLE_REFLECTION").unwrap_or_else(|_| "true".to_string()).parse().unwrap_or(true),
            node_id,
            otlp_endpoint: std::env::var("OTLP_ENDPOINT").ok(),
            jaeger_endpoint: std::env::var("JAEGER_ENDPOINT").ok(),
            xray_endpoint: std::env::var("XRAY_ENDPOINT").ok(),
            tracing_enabled,
            tracing_exporter,
            signing_config: None,
            cache_ttl_seconds,
            genesis_config_path,
        })
    }
}
