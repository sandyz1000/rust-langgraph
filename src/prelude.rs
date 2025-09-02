//! Prelude module - convenient imports for LangGraph users

// Core graph building types
pub use crate::core::{
    CompiledGraph, ExecutionContext, GraphConfig, GraphResult, GraphState, LangGraphError,
    NodeFunction, StateGraph, StreamEvent, StreamEventType, END, START,
};

// Channel types
pub use crate::core::channels::{
    AccumulatorChannel, BinaryOpChannel, Channel, EphemeralChannel, LastValueChannel,
};

// Pregel execution
pub use crate::core::pregel::{PregelEngine, TaskScheduler};

// Checkpointing
pub use crate::checkpoint::{CheckpointTuple, Checkpointer, InMemoryCheckpointer};

// Runtime
pub use crate::runtime::{
    ContextBuilder, ContextValue, LangGraphRuntime, RuntimeConfig, RuntimeContext, RuntimeExecutor,
    RuntimeMetrics,
};

// Prebuilt agents and tools
pub use crate::prebuilt::{
    create_react_agent, AgentAction, AgentMessage, AgentResponse, AgentState, AgentStep,
    CalculatorTool, MessageRole, ReactAgentBuilder, ReactPromptTemplate, Tool, ToolCall,
    WebSearchTool, LLM,
};

// External re-exports for convenience
pub use anyhow::{Error as AnyhowError, Result as AnyhowResult};
pub use async_trait::async_trait;
pub use serde::{Deserialize, Serialize};
pub use tokio;
pub use uuid::Uuid;

// Common type aliases
pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;
pub type DynError = Box<dyn std::error::Error + Send + Sync>;

// Streaming utilities
pub use futures::{Stream, StreamExt};

// JSON utilities
pub use serde_json::{json, Value as JsonValue};
