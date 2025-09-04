//! Error types for LangGraph Observability

use thiserror::Error;

/// Result type for observability operations
pub type ObservabilityResult<T> = Result<T, ObservabilityError>;

/// Error types for observability operations
#[derive(Error, Debug)]
pub enum ObservabilityError {
    /// Storage related errors
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Tracing related errors
    #[error("Tracing error: {0}")]
    Tracing(String),

    /// Metrics related errors
    #[error("Metrics error: {0}")]
    Metrics(String),

    /// Dashboard related errors
    #[error("Dashboard error: {0}")]
    Dashboard(String),

    /// Event bus related errors
    #[error("Event bus error: {0}")]
    EventBus(String),

    /// Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Database errors
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// IO errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// HTTP errors
    #[error("HTTP error: {0}")]
    Http(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Authentication errors
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Generic errors
    #[error("Error: {0}")]
    Generic(String),
}

/// Storage specific errors
#[derive(Error, Debug)]
pub enum StorageError {
    /// Connection failed
    #[error("Failed to connect to storage: {0}")]
    ConnectionFailed(String),

    /// Migration failed
    #[error("Failed to run migrations: {0}")]
    MigrationFailed(String),

    /// Query failed
    #[error("Query failed: {0}")]
    QueryFailed(String),

    /// Data not found
    #[error("Data not found: {0}")]
    NotFound(String),

    /// Data already exists
    #[error("Data already exists: {0}")]
    AlreadyExists(String),

    /// Constraint violation
    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<anyhow::Error> for ObservabilityError {
    fn from(err: anyhow::Error) -> Self {
        ObservabilityError::Generic(err.to_string())
    }
}
