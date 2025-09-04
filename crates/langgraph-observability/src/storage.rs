//! Storage backends for observability data

use crate::error::{ObservabilityError, ObservabilityResult, StorageError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[cfg(feature = "storage")]
use sqlx::{query, PgPool, Row, SqlitePool};

/// Trait for observability data storage
#[async_trait]
pub trait ObservabilityStorage: Send + Sync {
    /// Store a graph run
    async fn store_run(&mut self, run: &GraphRun<serde_json::Value>) -> ObservabilityResult<()>;

    /// Get a graph run by ID
    async fn get_run(
        &self,
        run_id: &str,
    ) -> ObservabilityResult<Option<GraphRun<serde_json::Value>>>;

    /// List graph runs with optional filters
    async fn list_runs(
        &self,
        filters: &RunFilters,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ObservabilityResult<Vec<GraphRun<serde_json::Value>>>;

    /// Store a trace span
    async fn store_span(&mut self, span: &TraceSpan) -> ObservabilityResult<()>;

    /// Get spans for a run
    async fn get_spans(&self, run_id: &str) -> ObservabilityResult<Vec<TraceSpan>>;

    /// Store prompt execution data
    async fn store_prompt(&mut self, prompt: &PromptExecution) -> ObservabilityResult<()>;

    /// Get prompt executions for a run
    async fn get_prompts(&self, run_id: &str) -> ObservabilityResult<Vec<PromptExecution>>;

    /// Store metrics data
    async fn store_metrics(&mut self, metrics: &MetricsSnapshot) -> ObservabilityResult<()>;

    /// Get metrics data
    async fn get_metrics(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> ObservabilityResult<Vec<MetricsSnapshot>>;
}

/// Graph execution run data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRun<S> {
    /// Unique run ID
    pub id: String,
    /// Graph ID/name
    pub graph_id: String,
    /// Run status
    pub status: RunStatus,
    /// Start time
    pub start_time: DateTime<Utc>,
    /// End time
    pub end_time: Option<DateTime<Utc>>,
    /// Initial state
    pub initial_state: S,
    /// Final state
    pub final_state: Option<S>,
    /// Error message if failed
    pub error: Option<String>,
    /// Run metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Configuration used for this run
    pub config: serde_json::Value,
}

impl<S> GraphRun<S> {
    pub fn new(id: String, graph_id: String) -> Self
    where
        S: Default,
    {
        Self {
            id,
            graph_id,
            status: RunStatus::Running,
            start_time: Utc::now(),
            end_time: None,
            initial_state: S::default(),
            final_state: None,
            error: None,
            metadata: HashMap::new(),
            config: serde_json::Value::Null,
        }
    }

    /// Calculate run duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        match self.end_time {
            Some(end) => (end - self.start_time).num_milliseconds() as u64,
            None => (Utc::now() - self.start_time).num_milliseconds() as u64,
        }
    }
}

/// Run status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RunStatus {
    /// Run is currently executing
    Running,
    /// Run completed successfully
    Completed,
    /// Run failed with an error
    Failed,
    /// Run was cancelled
    Cancelled,
}

/// Filters for querying runs
#[derive(Debug, Clone, Default)]
pub struct RunFilters {
    /// Filter by graph ID
    pub graph_id: Option<String>,
    /// Filter by status
    pub status: Option<RunStatus>,
    /// Filter by start time range
    pub start_time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Filter by metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Trace span data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    /// Span ID
    pub span_id: String,
    /// Parent span ID
    pub parent_span_id: Option<String>,
    /// Trace ID (typically the run ID)
    pub trace_id: String,
    /// Span name
    pub name: String,
    /// Start time
    pub start_time: DateTime<Utc>,
    /// End time
    pub end_time: Option<DateTime<Utc>>,
    /// Span attributes
    pub attributes: HashMap<String, serde_json::Value>,
    /// Span events
    pub events: Vec<SpanEvent>,
    /// Span status
    pub status: SpanStatus,
}

/// Span event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Event name
    pub name: String,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Event attributes
    pub attributes: HashMap<String, serde_json::Value>,
}

/// Span status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpanStatus {
    /// Span completed successfully
    Ok,
    /// Span failed with an error
    Error { message: String },
    /// Span status is unset
    Unset,
}

/// Prompt execution data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptExecution {
    /// Unique prompt execution ID
    pub id: String,
    /// Associated run ID
    pub run_id: String,
    /// Node ID that executed the prompt
    pub node_id: String,
    /// Model used
    pub model: String,
    /// Prompt input
    pub input: String,
    /// Model output
    pub output: String,
    /// Token usage
    pub token_usage: TokenUsage,
    /// Execution start time
    pub start_time: DateTime<Utc>,
    /// Execution end time
    pub end_time: DateTime<Utc>,
    /// Execution metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens
    pub input_tokens: u32,
    /// Output tokens
    pub output_tokens: u32,
    /// Total tokens
    pub total_tokens: u32,
}

/// Metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Timestamp of the snapshot
    pub timestamp: DateTime<Utc>,
    /// Metrics data
    pub metrics: HashMap<String, f64>,
}

/// In-memory storage implementation
pub struct InMemoryStorage {
    runs: Arc<RwLock<HashMap<String, GraphRun<serde_json::Value>>>>,
    spans: Arc<RwLock<HashMap<String, Vec<TraceSpan>>>>,
    prompts: Arc<RwLock<HashMap<String, Vec<PromptExecution>>>>,
    metrics: Arc<RwLock<Vec<MetricsSnapshot>>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            runs: Arc::new(RwLock::new(HashMap::new())),
            spans: Arc::new(RwLock::new(HashMap::new())),
            prompts: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl ObservabilityStorage for InMemoryStorage {
    async fn store_run(&mut self, run: &GraphRun<serde_json::Value>) -> ObservabilityResult<()> {
        let mut runs = self.runs.write().await;
        runs.insert(run.id.clone(), run.clone());
        Ok(())
    }

    async fn get_run(
        &self,
        run_id: &str,
    ) -> ObservabilityResult<Option<GraphRun<serde_json::Value>>> {
        let runs = self.runs.read().await;
        Ok(runs.get(run_id).cloned())
    }

    async fn list_runs(
        &self,
        filters: &RunFilters,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ObservabilityResult<Vec<GraphRun<serde_json::Value>>> {
        let runs = self.runs.read().await;
        let mut filtered_runs: Vec<_> = runs
            .values()
            .filter(|run| {
                // Apply filters
                if let Some(ref graph_id) = filters.graph_id {
                    if &run.graph_id != graph_id {
                        return false;
                    }
                }
                if let Some(ref status) = filters.status {
                    if &run.status != status {
                        return false;
                    }
                }
                if let Some((start, end)) = filters.start_time_range {
                    if run.start_time < start || run.start_time > end {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort by start time (newest first)
        filtered_runs.sort_by(|a, b| b.start_time.cmp(&a.start_time));

        // Apply offset and limit
        let start_idx = offset.unwrap_or(0) as usize;
        let end_idx = limit
            .map(|l| start_idx + l as usize)
            .unwrap_or(filtered_runs.len());

        Ok(filtered_runs
            .into_iter()
            .skip(start_idx)
            .take(end_idx - start_idx)
            .collect())
    }

    async fn store_span(&mut self, span: &TraceSpan) -> ObservabilityResult<()> {
        let mut spans = self.spans.write().await;
        spans
            .entry(span.trace_id.clone())
            .or_insert_with(Vec::new)
            .push(span.clone());
        Ok(())
    }

    async fn get_spans(&self, run_id: &str) -> ObservabilityResult<Vec<TraceSpan>> {
        let spans = self.spans.read().await;
        Ok(spans.get(run_id).cloned().unwrap_or_default())
    }

    async fn store_prompt(&mut self, prompt: &PromptExecution) -> ObservabilityResult<()> {
        let mut prompts = self.prompts.write().await;
        prompts
            .entry(prompt.run_id.clone())
            .or_insert_with(Vec::new)
            .push(prompt.clone());
        Ok(())
    }

    async fn get_prompts(&self, run_id: &str) -> ObservabilityResult<Vec<PromptExecution>> {
        let prompts = self.prompts.read().await;
        Ok(prompts.get(run_id).cloned().unwrap_or_default())
    }

    async fn store_metrics(&mut self, metrics: &MetricsSnapshot) -> ObservabilityResult<()> {
        let mut metrics_vec = self.metrics.write().await;
        metrics_vec.push(metrics.clone());
        Ok(())
    }

    async fn get_metrics(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> ObservabilityResult<Vec<MetricsSnapshot>> {
        let metrics = self.metrics.read().await;
        Ok(metrics
            .iter()
            .filter(|m| m.timestamp >= start_time && m.timestamp <= end_time)
            .cloned()
            .collect())
    }
}

/// SQLite storage implementation
#[cfg(feature = "storage")]
pub struct SqliteStorage {
    pool: sqlx::SqlitePool,
}

impl SqliteStorage {
    pub async fn new(database_path: &str) -> ObservabilityResult<Self> {
        let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}", database_path)).await?;

        // Run migrations (disabled for now due to compile-time check requirements)
        // TODO: Re-enable when DATABASE_URL is available during build
        // sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl ObservabilityStorage for SqliteStorage {
    async fn store_run(&mut self, run: &GraphRun<serde_json::Value>) -> ObservabilityResult<()> {
        let initial_state = serde_json::to_string(&run.initial_state)?;
        let final_state = match &run.final_state {
            Some(state) => Some(serde_json::to_string(state)?),
            None => None,
        };
        let metadata = serde_json::to_string(&run.metadata)?;
        let config = serde_json::to_string(&run.config)?;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO graph_runs 
            (id, graph_id, status, start_time, end_time, initial_state, final_state, error, metadata, config)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&run.id)
        .bind(&run.graph_id)
        .bind(run.status as i32)
        .bind(&run.start_time)
        .bind(&run.end_time)
        .bind(&initial_state)
        .bind(&final_state)
        .bind(&run.error)
        .bind(&metadata)
        .bind(&config)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_run(
        &self,
        run_id: &str,
    ) -> ObservabilityResult<Option<GraphRun<serde_json::Value>>> {
        let row = sqlx::query("SELECT * FROM graph_runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => {
                let status_int: i32 = row.get("status");
                let status = match status_int {
                    0 => RunStatus::Running,
                    1 => RunStatus::Completed,
                    2 => RunStatus::Failed,
                    3 => RunStatus::Cancelled,
                    _ => RunStatus::Running,
                };

                let initial_state_str: String = row.get("initial_state");
                let initial_state = serde_json::from_str(&initial_state_str)?;
                let final_state_opt: Option<String> = row.get("final_state");
                let final_state = match final_state_opt {
                    Some(s) => Some(serde_json::from_str(&s)?),
                    None => None,
                };
                let metadata_str: String = row.get("metadata");
                let metadata = serde_json::from_str(&metadata_str)?;
                let config_str: String = row.get("config");
                let config = serde_json::from_str(&config_str)?;

                Ok(Some(GraphRun {
                    id: row.get("id"),
                    graph_id: row.get("graph_id"),
                    status,
                    start_time: row.get("start_time"),
                    end_time: row.get("end_time"),
                    initial_state,
                    final_state,
                    error: row.get("error"),
                    metadata,
                    config,
                }))
            }
            None => Ok(None),
        }
    }

    async fn list_runs(
        &self,
        filters: &RunFilters,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ObservabilityResult<Vec<GraphRun<serde_json::Value>>> {
        // Build the query with filters
        let mut query = "SELECT * FROM graph_runs WHERE 1=1".to_string();
        let mut params: Vec<String> = vec![];

        if let Some(ref graph_id) = filters.graph_id {
            query.push_str(" AND graph_id = ?");
            params.push(graph_id.clone());
        }

        if let Some(ref status) = filters.status {
            query.push_str(" AND status = ?");
            params.push((*status as i32).to_string());
        }

        query.push_str(" ORDER BY start_time DESC");

        if let Some(limit) = limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        // This is a simplified implementation - in a real implementation,
        // you'd want to use proper parameter binding
        let rows = sqlx::query(&query).fetch_all(&self.pool).await?;

        let mut runs = Vec::new();
        for row in rows {
            // Extract row data and convert to GraphRun
            // This is a simplified implementation
        }

        Ok(runs)
    }

    async fn store_span(&mut self, span: &TraceSpan) -> ObservabilityResult<()> {
        // Implementation for storing spans in SQLite
        todo!("Implement SQLite span storage")
    }

    async fn get_spans(&self, run_id: &str) -> ObservabilityResult<Vec<TraceSpan>> {
        // Implementation for retrieving spans from SQLite
        todo!("Implement SQLite span retrieval")
    }

    async fn store_prompt(&mut self, prompt: &PromptExecution) -> ObservabilityResult<()> {
        // Implementation for storing prompts in SQLite
        todo!("Implement SQLite prompt storage")
    }

    async fn get_prompts(&self, run_id: &str) -> ObservabilityResult<Vec<PromptExecution>> {
        // Implementation for retrieving prompts from SQLite
        todo!("Implement SQLite prompt retrieval")
    }

    async fn store_metrics(&mut self, metrics: &MetricsSnapshot) -> ObservabilityResult<()> {
        // Implementation for storing metrics in SQLite
        todo!("Implement SQLite metrics storage")
    }

    async fn get_metrics(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> ObservabilityResult<Vec<MetricsSnapshot>> {
        // Implementation for retrieving metrics from SQLite
        todo!("Implement SQLite metrics retrieval")
    }
}

/// PostgreSQL storage implementation
pub struct PostgresStorage {
    pool: sqlx::PgPool,
}

impl PostgresStorage {
    pub async fn new(database_url: &str) -> ObservabilityResult<Self> {
        let pool = sqlx::PgPool::connect(database_url).await?;

        // Run migrations (disabled for now due to compile-time check requirements)
        // TODO: Re-enable when DATABASE_URL is available during build
        // sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl ObservabilityStorage for PostgresStorage {
    async fn store_run(&mut self, run: &GraphRun<serde_json::Value>) -> ObservabilityResult<()> {
        // Implementation for storing runs in PostgreSQL
        todo!("Implement PostgreSQL run storage")
    }

    async fn get_run(
        &self,
        run_id: &str,
    ) -> ObservabilityResult<Option<GraphRun<serde_json::Value>>> {
        // Implementation for retrieving runs from PostgreSQL
        todo!("Implement PostgreSQL run retrieval")
    }

    async fn list_runs(
        &self,
        filters: &RunFilters,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ObservabilityResult<Vec<GraphRun<serde_json::Value>>> {
        // Implementation for listing runs from PostgreSQL
        todo!("Implement PostgreSQL run listing")
    }

    async fn store_span(&mut self, span: &TraceSpan) -> ObservabilityResult<()> {
        // Implementation for storing spans in PostgreSQL
        todo!("Implement PostgreSQL span storage")
    }

    async fn get_spans(&self, run_id: &str) -> ObservabilityResult<Vec<TraceSpan>> {
        // Implementation for retrieving spans from PostgreSQL
        todo!("Implement PostgreSQL span retrieval")
    }

    async fn store_prompt(&mut self, prompt: &PromptExecution) -> ObservabilityResult<()> {
        // Implementation for storing prompts in PostgreSQL
        todo!("Implement PostgreSQL prompt storage")
    }

    async fn get_prompts(&self, run_id: &str) -> ObservabilityResult<Vec<PromptExecution>> {
        // Implementation for retrieving prompts from PostgreSQL
        todo!("Implement PostgreSQL prompt retrieval")
    }

    async fn store_metrics(&mut self, metrics: &MetricsSnapshot) -> ObservabilityResult<()> {
        // Implementation for storing metrics in PostgreSQL
        todo!("Implement PostgreSQL metrics storage")
    }

    async fn get_metrics(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> ObservabilityResult<Vec<MetricsSnapshot>> {
        // Implementation for retrieving metrics from PostgreSQL
        todo!("Implement PostgreSQL metrics retrieval")
    }
}
