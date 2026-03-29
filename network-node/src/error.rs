use thiserror::Error;

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Server error: {0}")]
    Server(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Shutdown timeout exceeded")]
    ShutdownTimeout,

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Cryptographic error: {0}")]
    Crypto(String),

    #[error("KMS error: {0}")]
    Kms(String),

    #[error("KMS timeout: {0}")]
    KmsTimeout(String),

    #[error("KMS rate limit: {0}")]
    KmsRateLimit(String),

    #[error("Signer error: {0}")]
    Signer(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Horizon client error: {0}")]
    HorizonClient(String),
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Pool exhausted")]
    PoolExhausted,

    #[error("Invalid connection string")]
    InvalidConnectionString,

    #[error("Migration failed: {0}")]
    MigrationFailed(String),
}

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Invalid request format: {0}")]
    InvalidFormat(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid value for field '{field}': {value}")]
    InvalidValue { field: String, value: String },

    #[error("Request size exceeds limit: {size} bytes")]
    RequestTooLarge { size: usize },
}

impl NetworkError {
    /// Check if this error should be retried
    pub fn is_retryable(&self) -> bool {
        match self {
            NetworkError::Database(db_err) => db_err.is_retryable(),
            NetworkError::Connection(_) => true,
            NetworkError::Server(_) => false,
            NetworkError::Validation(_) => false,
            NetworkError::Config(_) => false,
            NetworkError::Io(_) => false,
            NetworkError::Serialization(_) => false,
            NetworkError::ShutdownTimeout => false,
            NetworkError::Cancelled => false,
            NetworkError::Internal(_) => false,
            NetworkError::Crypto(_) => false,
            NetworkError::Kms(_) => true,
            NetworkError::KmsTimeout(_) => true,
            NetworkError::KmsRateLimit(_) => true,
            NetworkError::Signer(_) => false,
            NetworkError::NotImplemented(_) => false,
            NetworkError::HorizonClient(_) => true,
        }
    }

    /// Get error code for API responses
    pub fn error_code(&self) -> &'static str {
        match self {
            NetworkError::Config(_) => "CONFIG_ERROR",
            NetworkError::Database(_) => "DATABASE_ERROR",
            NetworkError::Server(_) => "SERVER_ERROR",
            NetworkError::Connection(_) => "CONNECTION_ERROR",
            NetworkError::Validation(_) => "VALIDATION_ERROR",
            NetworkError::Io(_) => "IO_ERROR",
            NetworkError::Serialization(_) => "SERIALIZATION_ERROR",
            NetworkError::ShutdownTimeout => "SHUTDOWN_TIMEOUT",
            NetworkError::Cancelled => "CANCELLED",
            NetworkError::Internal(_) => "INTERNAL_ERROR",
            NetworkError::Crypto(_) => "CRYPTO_ERROR",
            NetworkError::Kms(_) => "KMS_ERROR",
            NetworkError::KmsTimeout(_) => "KMS_TIMEOUT",
            NetworkError::KmsRateLimit(_) => "KMS_RATE_LIMIT",
            NetworkError::Signer(_) => "SIGNER_ERROR",
            NetworkError::NotImplemented(_) => "NOT_IMPLEMENTED",
            NetworkError::HorizonClient(_) => "HORIZON_CLIENT_ERROR",
        }
    }

    /// Get HTTP status code for this error
    pub fn http_status_code(&self) -> u16 {
        match self {
            NetworkError::Validation(_) => 400,
            NetworkError::Config(_) => 500,
            NetworkError::Database(_) => 500,
            NetworkError::Server(_) => 500,
            NetworkError::Connection(_) => 503,
            NetworkError::Io(_) => 500,
            NetworkError::Serialization(_) => 400,
            NetworkError::ShutdownTimeout => 503,
            NetworkError::Cancelled => 503,
            NetworkError::Internal(_) => 500,
            NetworkError::Crypto(_) => 400,
            NetworkError::Kms(_) => 502,
            NetworkError::KmsTimeout(_) => 504,
            NetworkError::KmsRateLimit(_) => 429,
            NetworkError::Signer(_) => 500,
            NetworkError::NotImplemented(_) => 501,
            NetworkError::HorizonClient(_) => 502,
        }
    }
}

impl DatabaseError {
    pub fn is_retryable(&self) -> bool {
        match self {
            DatabaseError::ConnectionFailed(_) => true,
            DatabaseError::QueryFailed(_) => false,
            DatabaseError::TransactionFailed(_) => false,
            DatabaseError::PoolExhausted => true,
            DatabaseError::InvalidConnectionString => false,
            DatabaseError::MigrationFailed(_) => false,
        }
    }
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, NetworkError>;

/// Error context for better error reporting
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub operation: String,
    pub component: String,
    pub request_id: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ErrorContext {
    pub fn new(operation: &str, component: &str) -> Self {
        Self {
            operation: operation.to_string(),
            component: component.to_string(),
            request_id: None,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }
}

/// Enhanced error with context
#[derive(Error, Debug)]
#[error("{error}")]
pub struct ContextualError {
    #[source]
    pub error: NetworkError,
    pub context: ErrorContext,
}

impl ContextualError {
    pub fn new(error: NetworkError, context: ErrorContext) -> Self {
        Self { error, context }
    }

    pub fn log_error(&self) {
        tracing::error!(
            operation = %self.context.operation,
            component = %self.context.component,
            request_id = ?self.context.request_id,
            error = %self.error,
            "Network error occurred"
        );
    }
}
