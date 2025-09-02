//! Runtime execution management

use langgraph_core::{GraphResult, LangGraphError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Runtime configuration for graph execution
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub max_concurrent_nodes: usize,
    pub default_timeout: std::time::Duration,
    pub enable_metrics: bool,
    pub debug_mode: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_nodes: 10,
            default_timeout: std::time::Duration::from_secs(300),
            enable_metrics: false,
            debug_mode: false,
        }
    }
}

/// Runtime metrics for monitoring
#[derive(Debug, Clone, Default)]
pub struct RuntimeMetrics {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub average_execution_time: std::time::Duration,
    pub node_execution_counts: HashMap<String, u64>,
}

/// LangGraph runtime for managing execution
#[derive(Debug)]
pub struct LangGraphRuntime {
    config: RuntimeConfig,
    metrics: Arc<RwLock<RuntimeMetrics>>,
}

impl LangGraphRuntime {
    /// Create a new runtime with default configuration
    pub fn new() -> Self {
        Self::with_config(RuntimeConfig::default())
    }

    /// Create a runtime with custom configuration
    pub fn with_config(config: RuntimeConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(RwLock::new(RuntimeMetrics::default())),
        }
    }

    /// Get runtime configuration
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Get runtime metrics
    pub async fn metrics(&self) -> RuntimeMetrics {
        self.metrics.read().await.clone()
    }

    /// Update metrics after execution
    pub async fn record_execution(
        &self,
        success: bool,
        duration: std::time::Duration,
        nodes_executed: Vec<String>,
    ) -> GraphResult<()> {
        let mut metrics = self.metrics.write().await;

        metrics.total_executions += 1;
        if success {
            metrics.successful_executions += 1;
        } else {
            metrics.failed_executions += 1;
        }

        // Update average execution time
        let total_time = metrics.average_execution_time.as_nanos() as f64
            * (metrics.total_executions - 1) as f64;
        let new_total = total_time + duration.as_nanos() as f64;
        metrics.average_execution_time =
            std::time::Duration::from_nanos((new_total / metrics.total_executions as f64) as u64);

        // Update node execution counts
        for node in nodes_executed {
            *metrics.node_execution_counts.entry(node).or_insert(0) += 1;
        }

        Ok(())
    }

    /// Reset metrics
    pub async fn reset_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        *metrics = RuntimeMetrics::default();
    }
}
