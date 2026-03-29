use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

use crate::config::NetworkConfig;
use crate::database::ConnectionPool;
use crate::error::{ContextualError, ErrorContext, NetworkError, Result};
use crate::error_middleware::ErrorMiddleware;
use crate::metrics::MetricsCollector;
use crate::rate_limiter::RateLimiter;
use crate::signing::SigningService;
use crate::stellar_service::StellarService;

/// Enhanced HTTP server with error middleware
pub struct EnhancedHttpServer {
    config: NetworkConfig,
    connection_pool: Arc<RwLock<ConnectionPool>>,
    error_middleware: Arc<ErrorMiddleware>,
    metrics_collector: Arc<MetricsCollector>,
    state_trie: Arc<RwLock<crate::state_trie::StateTrie>>,
    p2p_manager: Arc<crate::p2p::P2PManager>,
    signing_service: Arc<SigningService>,
    stellar_service: Arc<StellarService>,
    is_accepting_connections: Arc<RwLock<bool>>,
    active_connections: Arc<RwLock<usize>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl EnhancedHttpServer {
    /// Create a new enhanced HTTP server
    pub fn new(
        config: NetworkConfig,
        connection_pool: Arc<RwLock<ConnectionPool>>,
        error_middleware: Arc<ErrorMiddleware>,
        metrics_collector: Arc<MetricsCollector>,
        state_trie: Arc<RwLock<crate::state_trie::StateTrie>>,
        p2p_manager: Arc<crate::p2p::P2PManager>,
        signing_service: Arc<SigningService>,
        stellar_service: Arc<StellarService>,
    ) -> Self {
        Self {
            config,
            connection_pool,
            error_middleware,
            metrics_collector,
            state_trie,
            p2p_manager,
            signing_service,
            stellar_service,
            is_accepting_connections: Arc::new(RwLock::new(true)),
            active_connections: Arc::new(RwLock::new(0)),
            shutdown_tx: None,
        }
    }
    
    /// Get a reference to the signing service
    pub fn signing_service(&self) -> &Arc<SigningService> {
        &self.signing_service
    }

    /// Start the enhanced HTTP server
    pub async fn start(&mut self) -> Result<tokio::task::JoinHandle<Result<()>>> {
        info!(
            "Starting enhanced HTTP server on {}",
            self.config.bind_address
        );

        let bind_addr = self.config.bind_address.clone();
        let is_accepting = self.is_accepting_connections.clone();
        let active_connections = self.active_connections.clone();
        let connection_pool = self.connection_pool.clone();
        let error_middleware = self.error_middleware.clone();
        let metrics_collector = self.metrics_collector.clone();
        // Initialize rate limiter (try Redis via REDIS_URL env, otherwise in-memory)
        let redis_url = std::env::var("REDIS_URL").ok();
        let rate_limit_per_minute = std::env::var("RATE_LIMIT_PER_MINUTE")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(100);
        let rate_limiter = Arc::new(RateLimiter::new(redis_url, rate_limit_per_minute).await);

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        // Build the Axum app with middleware
        let app = axum::Router::new()
            .route("/health", axum::routing::get(health_check))
            .route("/ready", axum::routing::get(ready_check))
            .route("/metrics", axum::routing::get(metrics_endpoint))
            .route("/error-stats", axum::routing::get(error_stats))
            .route(
                "/circuit-breaker-status",
                axum::routing::get(circuit_breaker_status),
            )
            .route("/health/liveness", axum::routing::get(health_liveness))
            .route("/health/readiness", axum::routing::get(health_readiness))
            .route("/stellar/account/:account_id", axum::routing::get(get_stellar_account))
            .route("/stellar/ledger/latest", axum::routing::get(get_latest_ledger))
            .route("/stellar/providers/status", axum::routing::get(get_horizon_providers_status))
            .route("/stellar/providers/switch", axum::routing::post(switch_horizon_provider))
            .layer(
                middleware::from_fn_with_state(
                    error_middleware.clone(),
                    error_handler_middleware,
                )
            )
            .layer(
                middleware::from_fn_with_state(
                    is_accepting.clone(),
                    connection_limiter_middleware,
                )
            )
            .layer(
                middleware::from_fn_with_state(
                    rate_limiter.clone(),
                    rate_limit_middleware,
                )
            )
            .layer(
                middleware::from_fn_with_state(
                    active_connections.clone(),
                    connection_tracker_middleware,
                )
            )
            .layer(TraceLayer::new_for_http())
            .with_state(AppState {
                connection_pool,
                error_middleware,
                is_accepting,
                active_connections,
                metrics_collector,
                rate_limiter: rate_limiter.clone(),
                stellar_service: self.stellar_service.clone(),
            });

        // Parse bind address
        let addr: std::net::SocketAddr = bind_addr
            .parse()
            .map_err(|e| NetworkError::Config(format!("Invalid bind address: {}", e)))?;

        // Start the server
        let handle = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .map_err(|e| NetworkError::Server(format!("Failed to bind to {}: {}", addr, e)))?;

            info!("HTTP server listening on {}", addr);

            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                    info!("HTTP server shutdown signal received");
                })
                .await
                .map_err(|e| NetworkError::Server(format!("HTTP server error: {}", e)))
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
}

/// Application state
#[derive(Clone)]
struct AppState {
    connection_pool: Arc<RwLock<ConnectionPool>>,
    error_middleware: Arc<ErrorMiddleware>,
    is_accepting: Arc<RwLock<bool>>,
    active_connections: Arc<RwLock<usize>>,
    metrics_collector: Arc<MetricsCollector>,
    rate_limiter: Arc<RateLimiter>,
    stellar_service: Arc<StellarService>,
}

/// Error handler middleware
async fn error_handler_middleware(
    State(error_middleware): State<Arc<ErrorMiddleware>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Create error context
    let context =
        ErrorContext::new("http_request", "http_server").with_request_id(request_id.clone());

    // Process the request
    let response = match next.run(request).await {
        Ok(response) => response,
        Err(err) => {
            // Convert various error types to NetworkError
            let network_error = match err.downcast::<NetworkError>() {
                Ok(network_err) => *network_err,
                Err(_) => NetworkError::Server(format!("Internal server error: {}", err)),
            };

            // Handle error through middleware
            let contextual_error = error_middleware.handle_error(network_error, context).await;

            // Log the error
            contextual_error.log_error();

            // Return appropriate error response
            create_error_response(&contextual_error)
        }
    };

    Ok(response)
}

/// Rate-limit middleware: enforces per-IP limits and returns 429 when exceeded.
async fn rate_limit_middleware(
    State(rate_limiter): State<Arc<RateLimiter>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Try to get client IP from headers typically set by proxies
    let ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            request
                .headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    match rate_limiter.allow(&ip).await {
        Ok(true) => Ok(next.run(request).await),
        Ok(false) => Ok((StatusCode::TOO_MANY_REQUESTS, Json(json!({"error":"rate limit exceeded"}))).into_response()),
        Err(e) => {
            tracing::error!("Rate limiter error: {}", e);
            Ok((StatusCode::INTERNAL_SERVER_ERROR, "rate limiter error").into_response())
        }
    }
}

/// Connection limiter middleware
async fn connection_limiter_middleware(
    State(is_accepting): State<Arc<RwLock<bool>>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let accepting = *is_accepting.read().await;

    if !accepting {
        return Ok((StatusCode::SERVICE_UNAVAILABLE, "Server is shutting down").into_response());
    }

    Ok(next.run(request).await)
}

/// Connection tracker middleware
async fn connection_tracker_middleware(
    State(active_connections): State<Arc<RwLock<usize>>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Increment active connections
    *active_connections.write().await += 1;

    // Process request
    let response = next.run(request).await;

    // Decrement active connections
    *active_connections.write().await -= 1;

    Ok(response)
}

/// Create error response from contextual error
fn create_error_response(error: &ContextualError) -> Response {
    let status_code = StatusCode::from_u16(error.error.http_status_code())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let error_response = json!({
        "error": {
            "code": error.error.error_code(),
            "message": error.error.to_string(),
            "request_id": error.context.request_id,
            "timestamp": error.context.timestamp.to_rfc3339(),
        }
    });

    (status_code, Json(error_response)).into_response()
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Liveness probe - returns 200 OK if the service is running
async fn health_liveness() -> impl IntoResponse {
    // Simple liveness check - just confirms the binary is running
    (
        StatusCode::OK,
        Json(json!({
            "status": "alive",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
}

/// Readiness probe - returns 200 OK only when fully ready to accept traffic
async fn health_readiness(State(state): State<AppState>) -> impl IntoResponse {
    let pool = state.connection_pool.read().await;

    // Check database connectivity (lightweight check)
    let database_ready = pool.health_check().await.unwrap_or(false);

    // Check if we're accepting connections
    let accepting_connections = *state.is_accepting.read().await;

    if database_ready && accepting_connections {
        (
            StatusCode::OK,
            Json(json!({
                "status": "ready",
                "database": "connected",
                "accepting_connections": true,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })),
        )
    } else {
        let mut details = Vec::new();
        if !database_ready {
            details.push("database not connected");
        }
        if !accepting_connections {
            details.push("not accepting connections");
        }

        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not ready",
                "reasons": details,
                "database": if database_ready { "connected" } else { "disconnected" },
                "accepting_connections": accepting_connections,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })),
        )
    }
}

/// Ready check endpoint
async fn ready_check(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let pool = state.connection_pool.read().await;

    let database_healthy = pool
        .health_check()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let active_connections = pool.active_connections().await;

    let ready = database_healthy && active_connections > 0;

    Ok(Json(json!({
        "ready": ready,
        "database_healthy": database_healthy,
        "active_connections": active_connections,
        "timestamp": chrono::Utc::now().to_rfc3331(),
    })))
}

/// Metrics endpoint - Prometheus format
async fn metrics_endpoint(State(state): State<AppState>) -> impl IntoResponse {
    // Update metrics
    state.metrics_collector.increment_requests();

    // Get Prometheus-formatted metrics
    let metrics = state.metrics_collector.get_prometheus_metrics();

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        metrics,
    )
}

/// Error statistics endpoint
async fn error_stats(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.error_middleware.get_error_stats().await;

    Json(json!({
        "total_errors": stats.total_errors,
        "errors_by_type": stats.errors_by_type,
        "errors_by_component": stats.errors_by_component,
        "recent_errors_count": stats.recent_errors.len(),
        "last_updated": stats.last_updated.to_rfc3339(),
    }))
}

/// Circuit breaker status endpoint
async fn circuit_breaker_status(State(state): State<AppState>) -> impl IntoResponse {
    let breakers = state.error_middleware.get_circuit_breaker_status().await;

    let status_map: std::collections::HashMap<String, String> = breakers
        .into_iter()
        .map(|(component, breaker)| {
            let state_str = match breaker.state {
                crate::error_middleware::CircuitBreakerState::Closed => "closed",
                crate::error_middleware::CircuitBreakerState::Open => "open",
                crate::error_middleware::CircuitBreakerState::HalfOpen => "half_open",
            };
            (component, state_str.to_string())
        })
        .collect();

    Json(json!({
        "circuit_breakers": status_map,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

/// Handler for /sync/snapshot?chunk=N
async fn sync_snapshot(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let chunk_index = params
        .get("chunk")
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);

    let trie = state.state_trie.read().await;
    match trie.get_snapshot_chunk(chunk_index) {
        Ok(chunk) => Json(json!({
            "chunk_index": chunk_index,
            "data": chunk,
            "root_hash": hex::encode(trie.root_hash()),
        }))
        .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Handler for P2P ping
async fn p2p_ping(
    State(state): State<AppState>,
    Json(peer_info): Json<crate::p2p::PeerInfo>,
) -> impl IntoResponse {
    match state.p2p_manager.handle_ping(peer_info) {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Handler for getting Stellar account information
async fn get_stellar_account(
    State(state): State<AppState>,
    axum::extract::Path(account_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.stellar_service.get_account(&account_id).await {
        Ok(account) => Json(account).into_response(),
        Err(e) => (
            StatusCode::from_u16(e.http_status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({
                "error": e.error_code(),
                "message": e.to_string()
            }))
        ).into_response(),
    }
}

/// Handler for getting latest Stellar ledger
async fn get_latest_ledger(
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.stellar_service.get_latest_ledger().await {
        Ok(ledger) => Json(ledger).into_response(),
        Err(e) => (
            StatusCode::from_u16(e.http_status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({
                "error": e.error_code(),
                "message": e.to_string()
            }))
        ).into_response(),
    }
}

/// Handler for getting Horizon providers status
async fn get_horizon_providers_status(
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.stellar_service.get_provider_status().await {
        Ok(providers) => Json(json!({
            "providers": providers.iter().map(|p| {
                json!({
                    "name": p.provider.name,
                    "url": p.provider.url,
                    "priority": p.provider.priority,
                    "is_healthy": p.is_healthy,
                    "failure_count": p.failure_count,
                    "circuit_state": format!("{:?}", p.circuit_state),
                    "last_health_check": p.last_health_check.map(|t| t.elapsed().as_secs()),
                })
            }).collect::<Vec<_>>(),
            "total_providers": providers.len(),
            "healthy_providers": providers.iter().filter(|p| p.is_healthy).count(),
        })).into_response(),
        Err(e) => (
            StatusCode::from_u16(e.http_status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({
                "error": e.error_code(),
                "message": e.to_string()
            }))
        ).into_response(),
    }
}

/// Handler for switching Horizon provider
async fn switch_horizon_provider(
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.stellar_service.switch_provider().await {
        Ok(provider) => Json(json!({
            "message": "Successfully switched to backup provider",
            "provider": {
                "name": provider.name,
                "url": provider.url,
                "priority": provider.priority,
            }
        })).into_response(),
        Err(e) => (
            StatusCode::from_u16(e.http_status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({
                "error": e.error_code(),
                "message": e.to_string()
            }))
        ).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DatabaseConfig;
    use crate::error_middleware::ErrorMiddlewareConfig;

    #[tokio::test]
    async fn test_enhanced_server_creation() {
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

        let metrics_collector = Arc::new(MetricsCollector::new());
        let state_trie = Arc::new(RwLock::new(
            crate::state_trie::StateTrie::new("./data/test_trie").unwrap(),
        ));
        let p2p_manager = Arc::new(crate::p2p::P2PManager::new([0u8; 32]));

        let server = EnhancedHttpServer::new(
            config,
            connection_pool,
            error_middleware,
            metrics_collector,
            state_trie,
            p2p_manager,
        );

        // Server should be created successfully
        assert!(server.is_accepting_connections.read().await);
        assert_eq!(*server.active_connections.read().await, 0);
    }
}
