use axionvera_network_node::NetworkNode;
use axionvera_network_node::config::TracingExporter;
use axionvera_network_node::telemetry;
use std::path::PathBuf;
use tracing::{error, info, Level};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use metrics::{describe_counter, describe_gauge, describe_histogram};
use metrics_exporter_prometheus::PrometheusBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize configuration first
    let config = axionvera_network_node::config::NetworkConfig::from_env()?;

    // ==========================================
    // METRICS EXPORTER SETUP
    // ==========================================
    info!("Starting Prometheus metrics exporter on 0.0.0.0:9090/metrics");
    PrometheusBuilder::new()
        // Spin up the secondary lightweight HTTP server on the dedicated port 9090
        .with_http_listener(([0, 0, 0, 0], 9090))
        .install()
        .expect("Failed to install Prometheus recorder");

    // Register metric descriptions for documentation and Prometheus scraping
    describe_counter!("grpc_requests_total", "Total number of gRPC requests received");
    describe_histogram!("grpc_request_duration_seconds", "gRPC request latency in seconds");
    describe_counter!("soroban_rpc_errors_total", "Total number of Soroban RPC errors encountered");
    describe_gauge!("indexer_last_ledger_processed", "The sequence number of the last ledger processed by the indexer");
    // ==========================================

    // Initialize OpenTelemetry if enabled
    let subscriber = if config.tracing_enabled {
        match config.tracing_exporter {
            TracingExporter::Jaeger => {
                info!("Initializing Jaeger tracing");
                telemetry::init_jaeger_tracing(&config)?
            }
            TracingExporter::XRay => {
                info!("Initializing AWS X-Ray tracing");
                telemetry::init_xray_tracing(&config)?
            }
            TracingExporter::Otlp => {
                info!("Initializing OTLP tracing");
                telemetry::init_tracing(&config)?
            }
            TracingExporter::None => {
                info!("Tracing disabled, using basic logging");
                init_basic_logging(&config)?
            }
        }
    } else {
        info!("Tracing disabled, using basic logging");
        init_basic_logging(&config)?
    };

    // Initialize the subscriber
    // Note: If using `tracing-subscriber`, we use global init.
    subscriber.init();

    info!(
        service.name = "axionvera-network-node",
        service.version = env!("CARGO_PKG_VERSION"),
        node_id = %config.node_id,
        environment = std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
        tracing_enabled = config.tracing_enabled,
        tracing_exporter = ?config.tracing_exporter,
        "Starting axionvera-network node with distributed tracing"
    );

    // Create and start the network node
    let node = NetworkNode::new(config).await?;

    if let Err(e) = node.start().await {
        error!("Network node failed: {}", e);
        // Ensure telemetry is properly shutdown
        telemetry::shutdown_tracer();
        std::process::exit(1);
    }

    info!("Network node shutdown complete");


    info!("Network node shutdown complete");


    // Shutdown OpenTelemetry tracer provider
    telemetry::shutdown_tracer();

    Ok(())
}

fn init_basic_logging(config: &axionvera_network_node::config::NetworkConfig) -> Result<Box<dyn tracing::Subscriber + Send + Sync>, Box<dyn std::error::Error>> {
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
    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(format!("axionvera_network_node={}", log_level))
            }),
        )
        .with(fmt_layer)
        .with(file_layer);

    Ok(Box::new(subscriber))
}