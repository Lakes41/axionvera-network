use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::{
    trace::{self, RandomIdGenerator, Sampler},
    Resource,
};
use opentelemetry_semantic_conventions as semcov;
use tracing::Subscriber;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer, Registry};

use crate::config::NetworkConfig;

/// Initialize OpenTelemetry tracing with the given configuration
pub fn init_tracing(config: &NetworkConfig) -> Result<Box<dyn Subscriber + Send + Sync>, Box<dyn std::error::Error>> {
    // Create a new OpenTelemetry tracer
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(config.otlp_endpoint.as_deref().unwrap_or("http://localhost:4317")),
        )
        .with_trace_config(
            trace::config()
                .with_sampler(Sampler::AlwaysOn)
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(Resource::new(vec![
                    semcov::resource::SERVICE_NAME.string("axionvera-network-node"),
                    semcov::resource::SERVICE_VERSION.string(env!("CARGO_PKG_VERSION")),
                    semcov::resource::SERVICE_INSTANCE_ID.string(config.node_id.clone()),
                    semcov::resource::DEPLOYMENT_ENVIRONMENT.string(
                        std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string())
                    ),
                ])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    // Create a tracing layer with OpenTelemetry
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    // Create the subscriber with multiple layers
    let subscriber = Registry::default()
        .with(telemetry)
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::from_default_env());

    Ok(Box::new(subscriber))
}

/// Initialize OpenTelemetry with Jaeger exporter
pub fn init_jaeger_tracing(config: &NetworkConfig) -> Result<Box<dyn Subscriber + Send + Sync>, Box<dyn std::error::Error>> {
    let tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_endpoint(config.jaeger_endpoint.as_deref().unwrap_or("localhost:6831"))
        .with_service_name("axionvera-network-node")
        .with_trace_config(
            trace::config()
                .with_sampler(Sampler::AlwaysOn)
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(Resource::new(vec![
                    semcov::resource::SERVICE_NAME.string("axionvera-network-node"),
                    semcov::resource::SERVICE_VERSION.string(env!("CARGO_PKG_VERSION")),
                    semcov::resource::SERVICE_INSTANCE_ID.string(config.node_id.clone()),
                ])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    let subscriber = Registry::default()
        .with(telemetry)
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::from_default_env());

    Ok(Box::new(subscriber))
}

/// Initialize OpenTelemetry with AWS X-Ray exporter
pub fn init_xray_tracing(config: &NetworkConfig) -> Result<Box<dyn Subscriber + Send + Sync>, Box<dyn std::error::Error>> {
    // For AWS X-Ray, we typically use OTLP exporter with X-Ray daemon
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(config.xray_endpoint.as_deref().unwrap_or("http://localhost:2000")),
        )
        .with_trace_config(
            trace::config()
                .with_sampler(Sampler::AlwaysOn)
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(Resource::new(vec![
                    semcov::resource::SERVICE_NAME.string("axionvera-network-node"),
                    semcov::resource::SERVICE_VERSION.string(env!("CARGO_PKG_VERSION")),
                    semcov::resource::SERVICE_INSTANCE_ID.string(config.node_id.clone()),
                    semcov::resource::CLOUD_PROVIDER.string("aws"),
                    semcov::resource::CLOUD_REGION.string(
                        std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string())
                    ),
                ])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    let subscriber = Registry::default()
        .with(telemetry)
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::from_default_env());

    Ok(Box::new(subscriber))
}

/// Shutdown OpenTelemetry tracer provider
pub fn shutdown_tracer() {
    global::shutdown_tracer_provider();
}

/// Extract traceparent from HTTP headers
pub fn extract_traceparent(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string())
}

/// Inject traceparent into HTTP headers
pub fn inject_traceparent(headers: &mut axum::http::HeaderMap) {
    use opentelemetry::propagation::Extractor;
    use opentelemetry::global;
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    
    let mut injector = opentelemetry::propagation::TextMapPropagator::new(
        opentelemetry::propagation::TraceContextPropagator::new(),
    );
    
    let current_cx = tracing::Span::current().context();
    injector.inject_context(&current_cx, &mut HeaderInjector(headers));
}

struct HeaderInjector<'a>(&'a mut axum::http::HeaderMap);

impl<'a> opentelemetry::propagation::Injector for HeaderInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        if let Ok(name) = axum::http::HeaderName::from_bytes(key.as_bytes()) {
            if let Ok(value) = axum::http::HeaderValue::from_str(&value) {
                self.0.insert(name, value);
            }
        }
    }
}

/// Extract traceparent from gRPC metadata
pub fn extract_traceparent_grpc(metadata: &tonic::metadata::MetadataMap) -> Option<String> {
    metadata
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string())
}

/// Inject traceparent into gRPC metadata
pub fn inject_traceparent_grpc(metadata: &mut tonic::metadata::MetadataMap) {
    use opentelemetry::propagation::Injector;
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    
    let mut injector = opentelemetry::propagation::TextMapPropagator::new(
        opentelemetry::propagation::TraceContextPropagator::new(),
    );
    
    let current_cx = tracing::Span::current().context();
    injector.inject_context(&current_cx, &mut MetadataInjector(metadata));
}

struct MetadataInjector<'a>(&'a mut tonic::metadata::MetadataMap);

impl<'a> opentelemetry::propagation::Injector for MetadataInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        if let Ok(name) = tonic::metadata::MetadataKey::from_bytes(key.as_bytes()) {
            if let Ok(value) = tonic::metadata::MetadataValue::from_str(&value) {
                self.0.insert(name, value);
            }
        }
    }
}
