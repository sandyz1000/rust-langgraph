//! Graph executor with runtime management

use crate::runtime::{LangGraphRuntime, RuntimeConfig};
use langgraph_checkpoint::Checkpointer;
#[cfg(test)]
use langgraph_core::ExecutionContext;
use langgraph_core::{
    CompiledGraph, GraphConfig, GraphResult, GraphState, LangGraphError, StreamEvent,
};
use std::sync::Arc;

/// Enhanced executor with runtime management
pub struct RuntimeExecutor<T>
where
    T: GraphState,
{
    graph: CompiledGraph<T>,
    runtime: Arc<LangGraphRuntime>,
    checkpointer: Option<Arc<dyn Checkpointer<T>>>,
}

impl<T> RuntimeExecutor<T>
where
    T: GraphState,
{
    /// Create a new runtime executor
    pub fn new(graph: CompiledGraph<T>) -> Self {
        Self {
            graph,
            runtime: Arc::new(LangGraphRuntime::new()),
            checkpointer: None,
        }
    }

    /// Create with custom runtime configuration
    pub fn with_runtime_config(graph: CompiledGraph<T>, config: RuntimeConfig) -> Self {
        Self {
            graph,
            runtime: Arc::new(LangGraphRuntime::with_config(config)),
            checkpointer: None,
        }
    }

    /// Set checkpointer for the executor
    pub fn with_checkpointer(mut self, checkpointer: Arc<dyn Checkpointer<T>>) -> Self {
        self.checkpointer = Some(checkpointer);
        self
    }

    /// Execute graph with runtime management
    pub async fn execute(&self, initial_state: T) -> GraphResult<T> {
        self.execute_with_config(initial_state, GraphConfig::default())
            .await
    }

    /// Execute with configuration and runtime tracking
    pub async fn execute_with_config(
        &self,
        initial_state: T,
        config: GraphConfig,
    ) -> GraphResult<T> {
        let start_time = std::time::Instant::now();

        // Execute the graph using the compiled graph's invoke method
        let result = self.graph.invoke_with_config(initial_state, config).await;

        let execution_time = start_time.elapsed();
        let success = result.is_ok();

        // Record execution metrics
        // For now, we'll use an empty nodes list since we don't track individual node execution here
        self.runtime
            .record_execution(success, execution_time, vec![])
            .await?;

        result
    }

    /// Stream execution with runtime tracking
    pub async fn stream(
        &self,
        initial_state: T,
    ) -> GraphResult<Box<dyn futures::Stream<Item = StreamEvent<T>> + Send + Unpin>> {
        self.stream_with_config(initial_state, GraphConfig::default())
            .await
    }

    /// Stream with configuration
    pub async fn stream_with_config(
        &self,
        _initial_state: T,
        _config: GraphConfig,
    ) -> GraphResult<Box<dyn futures::Stream<Item = StreamEvent<T>> + Send + Unpin>> {
        // Temporary implementation - return error until properly implemented
        Err(LangGraphError::runtime(
            "Stream executor not fully implemented".to_string(),
        ))
    }

    /// Get runtime metrics
    pub async fn metrics(&self) -> crate::runtime::RuntimeMetrics {
        self.runtime.metrics().await
    }

    /// Reset runtime metrics
    pub async fn reset_metrics(&self) {
        self.runtime.reset_metrics().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use langgraph_core::{StateGraph, END, START};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestState {
        value: i32,
    }

    async fn increment_node(
        state: TestState,
        _context: ExecutionContext,
    ) -> GraphResult<TestState> {
        Ok(TestState {
            value: state.value + 1,
        })
    }

    #[tokio::test]
    async fn test_runtime_executor() {
        let mut graph = StateGraph::<TestState>::new();
        graph.add_node("increment", increment_node).unwrap();
        graph.add_edge(START, "increment").unwrap();
        graph.add_edge("increment", END).unwrap();

        let compiled = graph.compile().await.unwrap();
        let executor = RuntimeExecutor::new(compiled);

        let initial_state = TestState { value: 0 };
        let result = executor.execute(initial_state).await.unwrap();

        assert_eq!(result.value, 1);

        // Check metrics
        let metrics = executor.metrics().await;
        assert_eq!(metrics.total_executions, 1);
        assert_eq!(metrics.successful_executions, 1);
    }
}
