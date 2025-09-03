//! Metrics collection for LangGraph applications

use crate::config::MetricsConfig;
use crate::error::{ObservabilityError, ObservabilityResult};
use crate::storage::MetricsSnapshot;
use chrono::{DateTime, Utc};
use metrics::{counter, gauge, histogram, Counter, Gauge, Histogram};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;

/// Metrics collector for LangGraph observability
pub struct MetricsCollector {
    config: MetricsConfig,
    metrics: Arc<RwLock<LangGraphMetrics>>,
    collection_task: Option<tokio::task::JoinHandle<()>>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub async fn new(config: MetricsConfig) -> ObservabilityResult<Self> {
        // Initialize Prometheus metrics
        Self::init_prometheus_metrics(&config)?;

        let metrics = Arc::new(RwLock::new(LangGraphMetrics::new()));

        // Start metrics collection task
        let collection_task = if config.enabled {
            Some(Self::start_collection_task(
                metrics.clone(),
                config.collection_interval_seconds,
            ))
        } else {
            None
        };

        Ok(Self {
            config,
            metrics,
            collection_task,
        })
    }

    /// Initialize Prometheus metrics (disabled for now)
    fn init_prometheus_metrics(_config: &MetricsConfig) -> ObservabilityResult<()> {
        // TODO: Add prometheus feature and implementation
        Ok(())
    }

    /// Start the metrics collection task
    fn start_collection_task(
        metrics: Arc<RwLock<LangGraphMetrics>>,
        interval_seconds: u64,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_seconds));

            loop {
                interval.tick().await;

                let mut metrics_guard = metrics.write().await;
                metrics_guard.collect_system_metrics().await;
            }
        })
    }

    /// Record a graph run start
    pub async fn record_run_start(&self, graph_id: &str) -> ObservabilityResult<()> {
        // For now just use basic metrics without labels
        // TODO: Fix metrics labels syntax
        // counter!("langgraph_runs_started_total", "graph_id" => graph_id).increment(1);

        let mut metrics = self.metrics.write().await;
        metrics.record_run_start(graph_id).await;

        Ok(())
    }

    /// Record a graph run completion
    pub async fn record_run_complete(
        &self,
        graph_id: &str,
        duration: Duration,
        success: bool,
    ) -> ObservabilityResult<()> {
        // For now just use basic metrics without labels
        // TODO: Fix metrics labels syntax

        let mut metrics = self.metrics.write().await;
        metrics
            .record_run_complete(graph_id, duration, success)
            .await;

        Ok(())
    }

    /// Record node execution
    pub async fn record_node_execution(
        &self,
        graph_id: &str,
        node_id: &str,
        duration: Duration,
        success: bool,
    ) -> ObservabilityResult<()> {
        // For now just use basic metrics without labels
        // TODO: Fix metrics labels syntax

        let mut metrics = self.metrics.write().await;
        metrics
            .record_node_execution(graph_id, node_id, duration, success)
            .await;

        Ok(())
    }

    /// Record prompt execution
    pub async fn record_prompt_execution(
        &self,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        duration: Duration,
    ) -> ObservabilityResult<()> {
        // For now just use basic metrics without labels
        // TODO: Fix metrics labels syntax

        let mut metrics = self.metrics.write().await;
        metrics
            .record_prompt_execution(model, input_tokens, output_tokens, duration)
            .await;

        Ok(())
    }

    /// Record checkpoint creation
    pub async fn record_checkpoint(&self, graph_id: &str) -> ObservabilityResult<()> {
        // For now just use basic metrics without labels
        // TODO: Fix metrics labels syntax

        Ok(())
    }

    /// Get current metrics snapshot
    pub async fn get_metrics_snapshot(&self) -> ObservabilityResult<MetricsSnapshot> {
        let metrics = self.metrics.read().await;
        Ok(metrics.to_snapshot().await)
    }

    /// Export Prometheus metrics (disabled for now)
    pub async fn export_prometheus_metrics(&self) -> ObservabilityResult<String> {
        // TODO: Add prometheus feature and implementation
        Ok("# Prometheus metrics export not available".to_string())
    }

    /// Shutdown the metrics collector
    pub async fn shutdown(&self) -> ObservabilityResult<()> {
        if let Some(task) = &self.collection_task {
            task.abort();
        }
        Ok(())
    }
}

/// Internal metrics storage
struct LangGraphMetrics {
    run_counts: HashMap<String, u64>,
    run_durations: HashMap<String, Vec<f64>>,
    node_counts: HashMap<(String, String), u64>,
    node_durations: HashMap<(String, String), Vec<f64>>,
    prompt_counts: HashMap<String, u64>,
    prompt_durations: HashMap<String, Vec<f64>>,
    token_counts: HashMap<String, (u64, u64)>, // (input, output)
    system_metrics: SystemMetrics,
}

impl LangGraphMetrics {
    fn new() -> Self {
        Self {
            run_counts: HashMap::new(),
            run_durations: HashMap::new(),
            node_counts: HashMap::new(),
            node_durations: HashMap::new(),
            prompt_counts: HashMap::new(),
            prompt_durations: HashMap::new(),
            token_counts: HashMap::new(),
            system_metrics: SystemMetrics::new(),
        }
    }

    async fn record_run_start(&mut self, graph_id: &str) {
        *self.run_counts.entry(graph_id.to_string()).or_insert(0) += 1;
    }

    async fn record_run_complete(&mut self, graph_id: &str, duration: Duration, _success: bool) {
        self.run_durations
            .entry(graph_id.to_string())
            .or_insert_with(Vec::new)
            .push(duration.as_secs_f64());
    }

    async fn record_node_execution(
        &mut self,
        graph_id: &str,
        node_id: &str,
        duration: Duration,
        _success: bool,
    ) {
        let key = (graph_id.to_string(), node_id.to_string());
        *self.node_counts.entry(key.clone()).or_insert(0) += 1;
        self.node_durations
            .entry(key)
            .or_insert_with(Vec::new)
            .push(duration.as_secs_f64());
    }

    async fn record_prompt_execution(
        &mut self,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        duration: Duration,
    ) {
        *self.prompt_counts.entry(model.to_string()).or_insert(0) += 1;
        self.prompt_durations
            .entry(model.to_string())
            .or_insert_with(Vec::new)
            .push(duration.as_secs_f64());

        let (input, output) = self.token_counts.entry(model.to_string()).or_insert((0, 0));
        *input += input_tokens as u64;
        *output += output_tokens as u64;
    }

    async fn collect_system_metrics(&mut self) {
        self.system_metrics.update().await;
    }

    async fn to_snapshot(&self) -> MetricsSnapshot {
        let mut metrics_map = HashMap::new();

        // Add run metrics
        for (graph_id, count) in &self.run_counts {
            metrics_map.insert(format!("runs_total_{}", graph_id), *count as f64);
        }

        // Add average run durations
        for (graph_id, durations) in &self.run_durations {
            if !durations.is_empty() {
                let avg = durations.iter().sum::<f64>() / durations.len() as f64;
                metrics_map.insert(format!("run_duration_avg_{}", graph_id), avg);
            }
        }

        // Add node metrics
        for ((graph_id, node_id), count) in &self.node_counts {
            metrics_map.insert(
                format!("nodes_total_{}_{}", graph_id, node_id),
                *count as f64,
            );
        }

        // Add prompt metrics
        for (model, count) in &self.prompt_counts {
            metrics_map.insert(format!("prompts_total_{}", model), *count as f64);
        }

        // Add token metrics
        for (model, (input, output)) in &self.token_counts {
            metrics_map.insert(format!("tokens_input_{}", model), *input as f64);
            metrics_map.insert(format!("tokens_output_{}", model), *output as f64);
        }

        // Add system metrics
        metrics_map.insert(
            "memory_usage_bytes".to_string(),
            self.system_metrics.memory_usage,
        );
        metrics_map.insert(
            "cpu_usage_percent".to_string(),
            self.system_metrics.cpu_usage,
        );

        MetricsSnapshot {
            timestamp: Utc::now(),
            metrics: metrics_map,
        }
    }
}

/// System metrics
struct SystemMetrics {
    memory_usage: f64,
    cpu_usage: f64,
    last_update: Instant,
}

impl SystemMetrics {
    fn new() -> Self {
        Self {
            memory_usage: 0.0,
            cpu_usage: 0.0,
            last_update: Instant::now(),
        }
    }

    async fn update(&mut self) {
        // Update system metrics (simplified implementation)
        // In a real implementation, you'd use system APIs or libraries like `sysinfo`
        self.memory_usage = Self::get_memory_usage();
        self.cpu_usage = Self::get_cpu_usage();
        self.last_update = Instant::now();

        // Update global gauges - commented out due to metrics macro issues
        // TODO: Fix metrics macro syntax
        // gauge!("langgraph_memory_usage_bytes").set(self.memory_usage);
        // gauge!("langgraph_cpu_usage_percent").set(self.cpu_usage);
    }

    fn get_memory_usage() -> f64 {
        // Simplified implementation - would use actual system calls
        0.0
    }

    fn get_cpu_usage() -> f64 {
        // Simplified implementation - would use actual system calls
        0.0
    }
}

/// Custom metrics that can be defined by users
pub struct CustomMetric {
    pub name: String,
    pub metric_type: MetricType,
    pub labels: HashMap<String, String>,
    pub value: f64,
}

/// Types of metrics
#[derive(Debug, Clone)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

impl MetricsCollector {
    /// Record a custom metric
    pub async fn record_custom_metric(&self, metric: CustomMetric) -> ObservabilityResult<()> {
        // TODO: Fix metrics macro syntax for labels
        // For now just record basic metrics without labels to get compilation working
        match metric.metric_type {
            MetricType::Counter => {
                // counter!(metric.name).increment(metric.value as u64);
            }
            MetricType::Gauge => {
                // gauge!(metric.name).set(metric.value);
            }
            MetricType::Histogram => {
                // histogram!(metric.name).record(metric.value);
            }
        }

        Ok(())
    }
}
