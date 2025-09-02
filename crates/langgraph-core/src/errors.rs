//! Error types for LangGraph core operations

use thiserror::Error;
use std::fmt;

/// Primary error type for LangGraph operations
#[derive(Error, Debug)]
pub enum LangGraphError {
    /// Node execution failed
    #[error("Node execution failed: {node} - {source}")]
    NodeExecution {
        node: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Graph validation failed
    #[error("Graph validation failed: {message}")]
    GraphValidation { message: String },

    /// Checkpointing failed
    #[error("Checkpointing failed: {source}")]
    Checkpoint {
        #[from]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Serialization/deserialization failed
    #[error("Serialization failed: {source}")]
    Serialization {
        #[from]
        source: serde_json::Error,
    },

    /// Graph execution was interrupted
    #[error("Graph execution interrupted at node: {node}, reason: {reason}")]
    Interrupted { node: String, reason: String },

    /// Graph recursion limit exceeded
    #[error("Graph recursion limit exceeded: {limit}")]
    RecursionLimit { limit: usize },

    /// Invalid state update
    #[error("Invalid state update: {message}")]
    InvalidUpdate { message: String },

    /// Node not found
    #[error("Node not found: {node}")]
    NodeNotFound { node: String },

    /// Channel operation failed
    #[error("Channel operation failed: {channel} - {operation}")]
    ChannelOperation { channel: String, operation: String },

    /// Runtime error
    #[error("Runtime error: {message}")]
    Runtime { message: String },

    /// Timeout error
    #[error("Operation timed out: {operation}")]
    Timeout { operation: String },

    /// Configuration error
    #[error("Configuration error: {message}")]
    Configuration { message: String },
}

/// Result type alias for LangGraph operations
pub type GraphResult<T> = Result<T, LangGraphError>;

/// Error code enumeration for structured error handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    NodeExecution = 1001,
    GraphValidation = 1002,
    Checkpoint = 1003,
    Serialization = 1004,
    Interrupted = 1005,
    RecursionLimit = 1006,
    InvalidUpdate = 1007,
    NodeNotFound = 1008,
    ChannelOperation = 1009,
    Runtime = 1010,
    Timeout = 1011,
    Configuration = 1012,
}

impl ErrorCode {
    /// Get the error code as an integer
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Get the error code from an integer
    pub fn from_u32(code: u32) -> Option<Self> {
        match code {
            1001 => Some(ErrorCode::NodeExecution),
            1002 => Some(ErrorCode::GraphValidation),
            1003 => Some(ErrorCode::Checkpoint),
            1004 => Some(ErrorCode::Serialization),
            1005 => Some(ErrorCode::Interrupted),
            1006 => Some(ErrorCode::RecursionLimit),
            1007 => Some(ErrorCode::InvalidUpdate),
            1008 => Some(ErrorCode::NodeNotFound),
            1009 => Some(ErrorCode::ChannelOperation),
            1010 => Some(ErrorCode::Runtime),
            1011 => Some(ErrorCode::Timeout),
            1012 => Some(ErrorCode::Configuration),
            _ => None,
        }
    }
}

impl LangGraphError {
    /// Get the error code for this error
    pub fn code(&self) -> ErrorCode {
        match self {
            LangGraphError::NodeExecution { .. } => ErrorCode::NodeExecution,
            LangGraphError::GraphValidation { .. } => ErrorCode::GraphValidation,
            LangGraphError::Checkpoint { .. } => ErrorCode::Checkpoint,
            LangGraphError::Serialization { .. } => ErrorCode::Serialization,
            LangGraphError::Interrupted { .. } => ErrorCode::Interrupted,
            LangGraphError::RecursionLimit { .. } => ErrorCode::RecursionLimit,
            LangGraphError::InvalidUpdate { .. } => ErrorCode::InvalidUpdate,
            LangGraphError::NodeNotFound { .. } => ErrorCode::NodeNotFound,
            LangGraphError::ChannelOperation { .. } => ErrorCode::ChannelOperation,
            LangGraphError::Runtime { .. } => ErrorCode::Runtime,
            LangGraphError::Timeout { .. } => ErrorCode::Timeout,
            LangGraphError::Configuration { .. } => ErrorCode::Configuration,
        }
    }

    /// Create a new node execution error
    pub fn node_execution(
        node: impl Into<String>,
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        LangGraphError::NodeExecution {
            node: node.into(),
            source: source.into(),
        }
    }

    /// Create a new graph validation error
    pub fn graph_validation(message: impl Into<String>) -> Self {
        LangGraphError::GraphValidation {
            message: message.into(),
        }
    }

    /// Create a new interrupted error
    pub fn interrupted(node: impl Into<String>, reason: impl Into<String>) -> Self {
        LangGraphError::Interrupted {
            node: node.into(),
            reason: reason.into(),
        }
    }

    /// Create a new recursion limit error
    pub fn recursion_limit(limit: usize) -> Self {
        LangGraphError::RecursionLimit { limit }
    }

    /// Create a new invalid update error
    pub fn invalid_update(message: impl Into<String>) -> Self {
        LangGraphError::InvalidUpdate {
            message: message.into(),
        }
    }

    /// Create a new node not found error
    pub fn node_not_found(node: impl Into<String>) -> Self {
        LangGraphError::NodeNotFound { node: node.into() }
    }

    /// Create a new channel operation error
    pub fn channel_operation(
        channel: impl Into<String>,
        operation: impl Into<String>,
    ) -> Self {
        LangGraphError::ChannelOperation {
            channel: channel.into(),
            operation: operation.into(),
        }
    }

    /// Create a new runtime error
    pub fn runtime(message: impl Into<String>) -> Self {
        LangGraphError::Runtime {
            message: message.into(),
        }
    }

    /// Create a new timeout error
    pub fn timeout(operation: impl Into<String>) -> Self {
        LangGraphError::Timeout {
            operation: operation.into(),
        }
    }

    /// Create a new configuration error
    pub fn configuration(message: impl Into<String>) -> Self {
        LangGraphError::Configuration {
            message: message.into(),
        }
    }
}
