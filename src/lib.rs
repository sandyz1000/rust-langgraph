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
    Command, CompiledGraph, ExecutionContext, GraphConfig, GraphResult, GraphState,
    InterruptInfo, InvokeOutcome, LangGraphError, NodeFunction, NodeOutput, StateGraph,
    StateUpdate, StreamEvent, StreamEventType, END, START,
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
    use serde_json::{Map, Value};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestState {
        value: i32,
    }

    async fn increment(state: TestState, _ctx: ExecutionContext) -> GraphResult<Map<String, Value>> {
        let mut update = Map::new();
        update.insert("value".to_string(), Value::from(state.value + 1));
        Ok(update)
    }

    async fn decide_route(
        state: TestState,
        _ctx: ExecutionContext,
    ) -> GraphResult<NodeOutput<TestState>> {
        let mut update = Map::new();
        update.insert("value".to_string(), Value::from(state.value + 1));
        Ok(NodeOutput::with_command(
            update,
            Command::Goto("route_a".to_string()),
        ))
    }

    async fn route_a(state: TestState, _ctx: ExecutionContext) -> GraphResult<Map<String, Value>> {
        let mut update = Map::new();
        update.insert("value".to_string(), Value::from(state.value + 10));
        Ok(update)
    }

    async fn finalizer(state: TestState, _ctx: ExecutionContext) -> GraphResult<Map<String, Value>> {
        let mut update = Map::new();
        update.insert("value".to_string(), Value::from(state.value + 1));
        Ok(update)
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

    #[tokio::test]
    async fn test_command_goto_routing() {
        let mut graph = StateGraph::<TestState>::new();
        graph.add_node("decide", decide_route).unwrap();
        graph.add_node("route_a", route_a).unwrap();
        graph.add_edge(START, "decide").unwrap();
        graph.add_edge("route_a", END).unwrap();
        graph.set_entry_point("decide").unwrap();

        let app = graph.compile().await.unwrap();
        let result = app.invoke(TestState { value: 1 }).await.unwrap();

        assert_eq!(result.value, 12);
    }

    #[tokio::test]
    async fn test_interrupt_and_resume_workflow() {
        let mut graph = StateGraph::<TestState>::new();
        graph.add_node("finalizer", finalizer).unwrap();
        graph.add_edge(START, "finalizer").unwrap();
        graph.add_edge("finalizer", END).unwrap();
        graph.set_entry_point("finalizer").unwrap();

        let app = graph.compile().await.unwrap();
        let thread_id = uuid::Uuid::new_v4().to_string();

        let interrupt_config = GraphConfig::new()
            .with_thread_id(thread_id.clone())
            .with_interrupt_before("finalizer");

        let interrupted = app
            .invoke_outcome_with_config(TestState { value: 0 }, interrupt_config)
            .await
            .unwrap();

        match interrupted {
            InvokeOutcome::Interrupted(info) => {
                assert_eq!(info.next_nodes, vec!["finalizer".to_string()]);
                assert_eq!(info.state.value, 0);
            }
            InvokeOutcome::Completed(_) => panic!("Expected interruption before finalizer"),
        }

        let resumed = app.resume(&thread_id).await.unwrap();
        match resumed {
            InvokeOutcome::Completed(state) => assert_eq!(state.value, 1),
            InvokeOutcome::Interrupted(_) => panic!("Expected resumed execution to complete"),
        }
    }
}
