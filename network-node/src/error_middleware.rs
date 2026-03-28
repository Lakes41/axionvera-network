use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::{ContextualError, ErrorContext, NetworkError, Result};

/// Centralized error handling middleware
pub struct ErrorMiddleware {
    config: ErrorMiddlewareConfig,
    error_stats: Arc<RwLock<ErrorStats>>,
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
}

/// Configuration for error middleware
#[derive(Debug, Clone)]
pub struct ErrorMiddlewareConfig {
    pub enable_circuit_breaker: bool,
    pub enable_rate_limiting: bool,
    pub max_errors_per_minute: usize,
    pub circuit_breaker_threshold: usize,
    pub circuit_breaker_timeout: std::time::Duration,
    pub enable_error_aggregation: bool,
    pub log_internal_errors: bool,
    pub expose_error_details: bool,
}

impl Default for ErrorMiddlewareConfig {
    fn default() -> Self {
        Self {
            enable_circuit_breaker: true,
            enable_rate_limiting: true,
            max_errors_per_minute: 100,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout: std::time::Duration::from_secs(60),
            enable_error_aggregation: true,
            log_internal_errors: true,
            expose_error_details: false,
        }
    }
}

/// Error statistics
#[derive(Debug, Default)]
pub struct ErrorStats {
    pub total_errors: u64,
    pub errors_by_type: HashMap<String, u64>,
    pub errors_by_component: HashMap<String, u64>,
    pub recent_errors: Vec<ErrorRecord>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Individual error record
#[derive(Debug, Clone)]
pub struct ErrorRecord {
    pub id: String,
    pub error_type: String,
    pub component: String,
    pub operation: String,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub request_id: Option<String>,
    pub stack_trace: Option<String>,
}

/// Circuit breaker state
#[derive(Debug)]
pub struct CircuitBreaker {
    pub state: CircuitBreakerState,
    pub failure_count: usize,
    pub last_failure_time: Option<chrono::DateTime<chrono::Utc>>,
    pub success_count: usize,
}

#[derive(Debug, Clone)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl ErrorMiddleware {
    /// Create new error middleware
    pub fn new(config: ErrorMiddlewareConfig) -> Self {
        Self {
            config,
            error_stats: Arc::new(RwLock::new(ErrorStats::default())),
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Handle an error with centralized processing
    pub async fn handle_error(
        &self,
        error: NetworkError,
        context: ErrorContext,
    ) -> ContextualError {
        let request_id = context.request_id.clone();
        let error_record = self.create_error_record(&error, &context).await;

        // Record error statistics
        self.record_error_stats(&error, &error_record).await;

        // Check circuit breaker
        if self.config.enable_circuit_breaker {
            self.check_circuit_breaker(&context.component).await;
        }

        // Log the error
        self.log_error(&error, &context, &error_record).await;

        // Create contextual error
        let contextual_error = ContextualError::new(error, context);

        // Send to monitoring/alerting systems
        self.send_to_monitoring(&contextual_error, &error_record)
            .await;

        contextual_error
    }

    /// Create error record from error and context
    async fn create_error_record(
        &self,
        error: &NetworkError,
        context: &ErrorContext,
    ) -> ErrorRecord {
        ErrorRecord {
            id: Uuid::new_v4().to_string(),
            error_type: error.error_code().to_string(),
            component: context.component.clone(),
            operation: context.operation.clone(),
            message: error.to_string(),
            timestamp: chrono::Utc::now(),
            request_id: context.request_id.clone(),
            stack_trace: self.get_stack_trace(error).await,
        }
    }

    /// Record error statistics
    async fn record_error_stats(&self, error: &NetworkError, error_record: &ErrorRecord) {
        let mut stats = self.error_stats.write().await;

        stats.total_errors += 1;
        stats.last_updated = chrono::Utc::now();

        // Count by error type
        *stats
            .errors_by_type
            .entry(error.error_code().to_string())
            .or_insert(0) += 1;

        // Count by component
        *stats
            .errors_by_component
            .entry(error_record.component.clone())
            .or_insert(0) += 1;

        // Add to recent errors (keep last 100)
        stats.recent_errors.push(error_record.clone());
        if stats.recent_errors.len() > 100 {
            stats.recent_errors.remove(0);
        }
    }

    /// Log error with appropriate level
    async fn log_error(
        &self,
        error: &NetworkError,
        context: &ErrorContext,
        error_record: &ErrorRecord,
    ) {
        let log_level = self.determine_log_level(error);

        match log_level {
            LogLevel::Error => {
                error!(
                    error_type = %error.error_code(),
                    component = %context.component,
                    operation = %context.operation,
                    request_id = ?context.request_id,
                    error_id = %error_record.id,
                    error = %error,
                    "Network error occurred"
                );
            }
            LogLevel::Warn => {
                warn!(
                    error_type = %error.error_code(),
                    component = %context.component,
                    operation = %context.operation,
                    request_id = ?context.request_id,
                    error_id = %error_record.id,
                    error = %error,
                    "Network warning occurred"
                );
            }
            LogLevel::Info => {
                info!(
                    error_type = %error.error_code(),
                    component = %context.component,
                    operation = %context.operation,
                    request_id = ?context.request_id,
                    error_id = %error_record.id,
                    error = %error,
                    "Network info occurred"
                );
            }
            LogLevel::Debug => {
                debug!(
                    error_type = %error.error_code(),
                    component = %context.component,
                    operation = %context.operation,
                    request_id = ?context.request_id,
                    error_id = %error_record.id,
                    error = %error,
                    "Network debug occurred"
                );
            }
        }

        // Log internal stack traces if enabled
        if self.config.log_internal_errors && error_record.stack_trace.is_some() {
            error!(
                error_id = %error_record.id,
                stack_trace = ?error_record.stack_trace,
                "Internal error stack trace"
            );
        }
    }

    /// Determine log level based on error type
    fn determine_log_level(&self, error: &NetworkError) -> LogLevel {
        match error {
            NetworkError::Database(_) => LogLevel::Error,
            NetworkError::Server(_) => LogLevel::Error,
            NetworkError::Connection(_) => LogLevel::Warn,
            NetworkError::Validation(_) => LogLevel::Info,
            NetworkError::Config(_) => LogLevel::Error,
            NetworkError::Io(_) => LogLevel::Error,
            NetworkError::Serialization(_) => LogLevel::Warn,
            NetworkError::ShutdownTimeout => LogLevel::Warn,
            NetworkError::Cancelled => LogLevel::Info,
            NetworkError::Internal(_) => LogLevel::Error,
        }
    }

    /// Get stack trace for error (in production, this might be limited)
    async fn get_stack_trace(&self, error: &NetworkError) -> Option<String> {
        if self.config.log_internal_errors {
            Some(format!("{:?}", error))
        } else {
            None
        }
    }

    /// Check and update circuit breaker
    async fn check_circuit_breaker(&self, component: &str) {
        let mut breakers = self.circuit_breakers.write().await;

        let breaker = breakers
            .entry(component.to_string())
            .or_insert(CircuitBreaker {
                state: CircuitBreakerState::Closed,
                failure_count: 0,
                last_failure_time: None,
                success_count: 0,
            });

        breaker.failure_count += 1;

        match breaker.state {
            CircuitBreakerState::Closed => {
                if breaker.failure_count >= self.config.circuit_breaker_threshold {
                    breaker.state = CircuitBreakerState::Open;
                    breaker.last_failure_time = Some(chrono::Utc::now());
                    warn!(
                        component = %component,
                        failure_count = breaker.failure_count,
                        "Circuit breaker opened for component"
                    );
                }
            }
            CircuitBreakerState::Open => {
                // Check if timeout has passed
                if let Some(last_failure) = breaker.last_failure_time {
                    let elapsed = chrono::Utc::now() - last_failure;
                    if elapsed
                        > chrono::Duration::from_std(self.config.circuit_breaker_timeout).unwrap()
                    {
                        breaker.state = CircuitBreakerState::HalfOpen;
                        info!(
                            component = %component,
                            "Circuit breaker moved to half-open state"
                        );
                    }
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Reset on failure in half-open state
                breaker.state = CircuitBreakerState::Open;
                breaker.last_failure_time = Some(chrono::Utc::now());
                warn!(
                    component = %component,
                    "Circuit breaker opened again from half-open state"
                );
            }
        }
    }

    /// Record success for circuit breaker
    pub async fn record_success(&self, component: &str) {
        if !self.config.enable_circuit_breaker {
            return;
        }

        let mut breakers = self.circuit_breakers.write().await;

        if let Some(breaker) = breakers.get_mut(component) {
            breaker.success_count += 1;

            match breaker.state {
                CircuitBreakerState::HalfOpen => {
                    // Close circuit breaker after sufficient successes
                    if breaker.success_count >= 3 {
                        breaker.state = CircuitBreakerState::Closed;
                        breaker.failure_count = 0;
                        breaker.success_count = 0;
                        info!(
                            component = %component,
                            "Circuit breaker closed after successful operations"
                        );
                    }
                }
                _ => {}
            }
        }
    }

    /// Check if circuit breaker allows operation
    pub async fn is_circuit_breaker_open(&self, component: &str) -> bool {
        if !self.config.enable_circuit_breaker {
            return false;
        }

        let breakers = self.circuit_breakers.read().await;

        if let Some(breaker) = breakers.get(component) {
            matches!(breaker.state, CircuitBreakerState::Open)
        } else {
            false
        }
    }

    /// Send error to monitoring systems
    async fn send_to_monitoring(
        &self,
        contextual_error: &ContextualError,
        error_record: &ErrorRecord,
    ) {
        // In a real implementation, this would send to monitoring systems
        // like Prometheus, Datadog, Sentry, etc.
        debug!(
            error_id = %error_record.id,
            "Sending error to monitoring systems"
        );
    }

    /// Get error statistics
    pub async fn get_error_stats(&self) -> ErrorStats {
        self.error_stats.read().await.clone()
    }

    /// Get circuit breaker status
    pub async fn get_circuit_breaker_status(&self) -> HashMap<String, CircuitBreaker> {
        self.circuit_breakers.read().await.clone()
    }

    /// Reset error statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.error_stats.write().await;
        *stats = ErrorStats::default();
        info!("Error statistics reset");
    }

    /// Reset circuit breaker for a component
    pub async fn reset_circuit_breaker(&self, component: &str) {
        let mut breakers = self.circuit_breakers.write().await;
        breakers.insert(
            component.to_string(),
            CircuitBreaker {
                state: CircuitBreakerState::Closed,
                failure_count: 0,
                last_failure_time: None,
                success_count: 0,
            },
        );
        info!(
            component = %component,
            "Circuit breaker reset"
        );
    }
}

#[derive(Debug)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

/// Error handling trait for components
#[async_trait::async_trait]
pub trait ErrorHandler {
    async fn handle_error(&self, error: NetworkError, context: ErrorContext) -> ContextualError;
}

/// Default error handler implementation
pub struct DefaultErrorHandler {
    middleware: Arc<ErrorMiddleware>,
}

impl DefaultErrorHandler {
    pub fn new(middleware: Arc<ErrorMiddleware>) -> Self {
        Self { middleware }
    }
}

#[async_trait::async_trait]
impl ErrorHandler for DefaultErrorHandler {
    async fn handle_error(&self, error: NetworkError, context: ErrorContext) -> ContextualError {
        self.middleware.handle_error(error, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DatabaseError;

    #[tokio::test]
    async fn test_error_middleware_basic() {
        let config = ErrorMiddlewareConfig::default();
        let middleware = ErrorMiddleware::new(config);

        let error = NetworkError::Database(DatabaseError::ConnectionFailed("test".to_string()));
        let context = ErrorContext::new("test_operation", "test_component");

        let contextual_error = middleware.handle_error(error, context).await;

        // Check that error was recorded
        let stats = middleware.get_error_stats().await;
        assert_eq!(stats.total_errors, 1);
        assert!(stats.errors_by_type.contains_key("DATABASE_ERROR"));
        assert!(stats.errors_by_component.contains_key("test_component"));
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let config = ErrorMiddlewareConfig {
            enable_circuit_breaker: true,
            circuit_breaker_threshold: 2,
            ..Default::default()
        };
        let middleware = ErrorMiddleware::new(config);

        let component = "test_component";

        // Should not be open initially
        assert!(!middleware.is_circuit_breaker_open(component).await);

        // Trigger errors to open circuit breaker
        for i in 0..3 {
            let error =
                NetworkError::Database(DatabaseError::ConnectionFailed(format!("test {}", i)));
            let context = ErrorContext::new("test_operation", component);
            middleware.handle_error(error, context).await;
        }

        // Should be open now
        assert!(middleware.is_circuit_breaker_open(component).await);

        // Reset circuit breaker
        middleware.reset_circuit_breaker(component).await;

        // Should not be open anymore
        assert!(!middleware.is_circuit_breaker_open(component).await);
    }

    #[tokio::test]
    async fn test_error_stats() {
        let middleware = ErrorMiddleware::new(ErrorMiddlewareConfig::default());

        // Generate some errors
        for i in 0..5 {
            let error = NetworkError::Validation(format!("validation error {}", i));
            let context = ErrorContext::new("test_operation", "test_component");
            middleware.handle_error(error, context).await;
        }

        let stats = middleware.get_error_stats().await;
        assert_eq!(stats.total_errors, 5);
        assert_eq!(stats.errors_by_type.get("VALIDATION_ERROR"), Some(&5));
        assert_eq!(stats.errors_by_component.get("test_component"), Some(&5));
        assert_eq!(stats.recent_errors.len(), 5);
    }
}
