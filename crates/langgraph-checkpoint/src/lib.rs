//! Checkpointing and persistence for LangGraph
//!
//! This crate provides checkpoint functionality for LangGraph, enabling
//! durable execution and resumability of graph workflows.

pub mod base;
pub mod memory;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "redis")]
pub mod redis;

pub use base::{Checkpointer, Checkpoint, CheckpointMetadata, CheckpointTuple, CheckpointConfig};
pub use memory::InMemoryCheckpointer;

#[cfg(feature = "sqlite")]
pub use sqlite::SqliteCheckpointer;

#[cfg(feature = "postgres")]
pub use postgres::PostgresCheckpointer;

#[cfg(feature = "redis")]
pub use redis::RedisCheckpointer;

/// Re-exports for convenience
pub use langgraph_core::{GraphResult, LangGraphError};
