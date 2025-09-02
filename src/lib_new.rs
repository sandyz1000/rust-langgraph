//! Rust LangGraph - A library for building stateful, multi-actor applications with LLMs
//! 
//! This is a comprehensive Rust implementation of LangGraph, providing the same
//! core functionality as the original Python version with Rust's performance,
//! safety, and concurrency benefits.

pub mod prelude;

// Re-export core functionality
pub use langgraph_core as core;
pub use langgraph_checkpoint as checkpoint;
pub use langgraph_runtime as runtime;
pub use langgraph_prebuilt as prebuilt;

// Re-export common types for convenience
pub use langgraph_core::{
    StateGraph, CompiledGraph, GraphResult, GraphError,
    ExecutionContext, ExecutionConfig, StreamEvent, StreamMode,
    GraphState, NodeFunction, START, END,
};

pub use langgraph_checkpoint::{BaseCheckpointer, InMemoryCheckpointer};
pub use langgraph_runtime::{RuntimeExecutor, LangGraphRuntime};
pub use langgraph_prebuilt::{
    ReactAgentBuilder, create_react_agent, AgentState, AgentMessage,
    Tool, LLM, CalculatorTool, WebSearchTool,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestState {
        value: i32,
    }
    
    async fn increment(state: TestState, _ctx: ExecutionContext) -> GraphResult<TestState> {
        Ok(TestState { value: state.value + 1 })
    }
    
    #[tokio::test]
    async fn test_basic_integration() {
        let mut graph = StateGraph::<TestState>::new();
        graph.add_node("increment", increment).unwrap();
        graph.add_edge(START, "increment").unwrap();
        graph.add_edge("increment", END).unwrap();
        
        let app = graph.compile().await.unwrap();
        let result = app.invoke(TestState { value: 0 }).await.unwrap();
        
        assert_eq!(result.value, 1);
    }
}
