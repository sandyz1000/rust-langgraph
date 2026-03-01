//! Prelude module - convenient imports for LangGraph users

// Core graph building types
pub use crate::core::{
    Command, CompiledGraph, END, ExecutionContext, GraphConfig, GraphResult, GraphState,
    InterruptInfo, InvokeOutcome, LangGraphError, NodeFunction, NodeOutput, START, StateGraph,
    StateUpdate, StreamEvent, StreamEventType,
};

// Channel types
pub use crate::core::channels::{
    AccumulatorChannel, BinaryOpChannel, BinaryOpReducer, Channel, ChannelSpec, ChannelType,
    EphemeralChannel, LastValueChannel, ManagedValueChannel,
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
    AgentAction, AgentMessage, AgentResponse, AgentState, AgentStep, CalculatorTool, LLM,
    MessageRole, ReactAgentBuilder, ReactPromptTemplate, Tool, ToolCall, WebSearchTool,
    create_react_agent,
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
pub use serde_json::{Value as JsonValue, json};
