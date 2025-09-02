//! Rust LangGraph - A library for building stateful, multi-actor applications with LLMs
//!
//! This is a comprehensive Rust implementation of LangGraph, providing the same
//! core functionality as the original Python version with Rust's performance,
//! safety, and concurrency benefits.

pub mod prelude;

// Re-export core functionality
pub use langgraph_checkpoint as checkpoint;
pub use langgraph_core as core;
pub use langgraph_prebuilt as prebuilt;
pub use langgraph_runtime as runtime;

// Re-export common types for convenience
pub use langgraph_core::{
    CompiledGraph, ExecutionContext, GraphConfig, GraphResult, GraphState, LangGraphError,
    NodeFunction, StateGraph, StreamEvent, StreamEventType, END, START,
};

pub use langgraph_checkpoint::{Checkpointer, InMemoryCheckpointer};
pub use langgraph_prebuilt::{
    create_react_agent, AgentMessage, AgentState, CalculatorTool, ReactAgentBuilder, Tool,
    WebSearchTool, LLM,
};
pub use langgraph_runtime::{LangGraphRuntime, RuntimeExecutor};

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestState {
        value: i32,
    }

    async fn increment(state: TestState, _ctx: ExecutionContext) -> GraphResult<TestState> {
        Ok(TestState {
            value: state.value + 1,
        })
    }

    #[tokio::test]
    async fn test_basic_integration() {
        let mut graph = StateGraph::<TestState>::new();
        graph.add_node("increment", increment).unwrap();
        graph.add_edge(START, "increment").unwrap();
        graph.add_edge("increment", END).unwrap();
        graph.set_entry_point("increment").unwrap();

        let app = graph.compile().await.unwrap();
        let result = app.invoke(TestState { value: 0 }).await.unwrap();

        assert_eq!(result.value, 1);
    }
}
