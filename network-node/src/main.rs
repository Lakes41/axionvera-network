use axionvera_network_node::NetworkNode;
use std::path::PathBuf;
use tracing::{error, info, Level};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let config = axionvera_network_node::config::NetworkConfig::from_env()?;

    let log_level = config.log_level.parse::<Level>().unwrap_or(Level::INFO);

    // Create JSON formatted logging layer
    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339());

    // Optional: File logging for production
    let log_dir = std::env::var("LOG_DIR").unwrap_or_else(|_| "logs".to_string());
    let file_appender = RollingFileAppender::new(
        Rotation::DAILY,
        PathBuf::from(&log_dir),
        "axionvera-network.log",
    );

    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(file_appender)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339());

    // Initialize subscriber with both console and file layers
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(format!("axionvera_network_node={}", log_level))
            }),
        )
        .with(fmt_layer)
        .with(file_layer)
        .init();

    info!(
        service.name = "axionvera-network-node",
        service.version = env!("CARGO_PKG_VERSION"),
        environment = std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
        "Starting axionvera-network node"
    );

    // Create and start the network node
    let node = NetworkNode::new(config).await?;

    if let Err(e) = node.start().await {
        error!("Network node failed: {}", e);
        std::process::exit(1);
    }

    info!("Network node shutdown complete");
    Ok(())
}
