//! Base checkpointing traits and types

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use langgraph_core::{GraphResult, GraphState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use serde_json::Value as JsonValue;

/// Checkpoint data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "S: for<'de2> Deserialize<'de2>")]
pub struct Checkpoint<S: GraphState> {
    /// Checkpoint version
    pub version: u32,
    /// Unique checkpoint ID
    pub id: String,
    /// Timestamp when checkpoint was created
    pub timestamp: DateTime<Utc>,
    /// Current graph state
    pub state: S,
    /// Step number in execution
    pub step: u32,
    /// Next nodes to execute
    pub next_nodes: Vec<String>,
    /// Channel values
    pub channel_values: HashMap<String, JsonValue>,
    /// Channel versions
    pub channel_versions: HashMap<String, u64>,
    /// Node execution history
    pub node_history: Vec<NodeExecution>,
    /// Checkpoint metadata
    pub metadata: CheckpointMetadata,
}

/// Metadata associated with a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    /// Source of the checkpoint
    pub source: CheckpointSource,
    /// Thread ID for conversation tracking
    pub thread_id: Option<String>,
    /// User ID who triggered the checkpoint
    pub user_id: Option<String>,
    /// Parent checkpoint IDs
    pub parents: HashMap<String, String>,
    /// Custom metadata fields
    pub custom: HashMap<String, JsonValue>,
}

/// Source of a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointSource {
    /// The checkpoint was created from an input to invoke/stream/batch.
    Input,
    /// Checkpoint from inside the graph execution loop
    Loop,
    /// Checkpoint from manual state update
    Update,
    /// Checkpoint created as a copy/fork of another checkpoint
    Fork,
    /// Checkpoint from error/interrupt
    Interrupt,
}

/// Node execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeExecution {
    /// Node name
    pub node_name: String,
    /// Execution start time
    pub started_at: DateTime<Utc>,
    /// Execution end time
    pub completed_at: Option<DateTime<Utc>>,
    /// Execution status
    pub status: ExecutionStatus,
    /// Input state for the node
    pub input_state: JsonValue,
    /// Output state from the node
    pub output_state: Option<JsonValue>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution duration in milliseconds
    pub duration_ms: Option<u64>,
}

/// Node execution status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// Node is pending execution
    Pending,
    /// Node is currently running
    Running,
    /// Node completed successfully
    Completed,
    /// Node failed with error
    Failed,
    /// Node was interrupted
    Interrupted,
}

/// Checkpoint tuple for storage operations
// TODO: Find or write the usage of CheckpointTuple,  
#[derive(Debug, Clone)]
pub struct CheckpointTuple<S: GraphState> {
    /// The checkpoint data
    pub checkpoint: Checkpoint<S>,
    /// Pending writes to apply
    pub pending_writes: Vec<PendingWrite>,
    /// Checkpoint configuration
    pub config: CheckpointConfig,
}

/// Pending write operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingWrite {
    /// Channel name
    pub channel: String,
    /// Operation type
    pub operation: WriteOperation,
    /// Value to write
    pub value: JsonValue,
}

/// Write operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WriteOperation {
    /// Set the channel value
    Set,
    /// Append to the channel value
    Append,
    /// Clear the channel value
    Clear,
}

/// Configuration for checkpointing
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// Thread ID for checkpoint isolation
    pub thread_id: String,
    /// Whether to enable compression
    pub compress: bool,
    /// Maximum checkpoints to retain
    pub max_checkpoints: Option<usize>,
    /// Whether to store full state or deltas
    pub store_deltas: bool,
    /// Custom configuration
    pub custom: HashMap<String, JsonValue>,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            thread_id: Uuid::new_v4().to_string(),
            compress: false,
            max_checkpoints: Some(100),
            store_deltas: false,
            custom: HashMap::new(),
        }
    }
}

/// Trait for checkpoint storage backends
#[async_trait]
pub trait Checkpointer<S: GraphState>: Send + Sync {
    /// Save a checkpoint
    async fn put(
        &self,
        config: &CheckpointConfig,
        checkpoint: Checkpoint<S>,
        metadata: CheckpointMetadata,
    ) -> GraphResult<()>;

    /// Retrieve a checkpoint by ID
    async fn get(
        &self,
        config: &CheckpointConfig,
        checkpoint_id: &str,
    ) -> GraphResult<Option<CheckpointTuple<S>>>;

    /// Get the latest checkpoint for a thread
    async fn get_latest(
        &self,
        config: &CheckpointConfig,
    ) -> GraphResult<Option<CheckpointTuple<S>>>;

    /// List checkpoints for a thread
    async fn list(
        &self,
        config: &CheckpointConfig,
        limit: Option<usize>,
        before: Option<&str>,
    ) -> GraphResult<Vec<CheckpointTuple<S>>>;

    /// Delete a checkpoint
    async fn delete(
        &self,
        config: &CheckpointConfig,
        checkpoint_id: &str,
    ) -> GraphResult<bool>;

    /// Clear all checkpoints for a thread
    async fn clear_thread(
        &self,
        thread_id: &str,
    ) -> GraphResult<usize>;

    /// Apply pending writes to a checkpoint
    async fn apply_writes(
        &self,
        checkpoint: &mut Checkpoint<S>,
        writes: &[PendingWrite],
    ) -> GraphResult<()> {
        for write in writes {
            match write.operation {
                WriteOperation::Set => {
                    checkpoint.channel_values.insert(write.channel.clone(), write.value.clone());
                }
                WriteOperation::Append => {
                    // Handle append operation based on channel type
                    let key = &write.channel;
                    if let Some(existing) = checkpoint.channel_values.get_mut(key) {
                        if let (Some(existing_array), Some(new_array)) = (
                            existing.as_array_mut(),
                            write.value.as_array(),
                        ) {
                            existing_array.extend(new_array.iter().cloned());
                        }
                    } else {
                        checkpoint.channel_values.insert(write.channel.clone(), write.value.clone());
                    }
                }
                WriteOperation::Clear => {
                    checkpoint.channel_values.remove(&write.channel);
                }
            }
        }
        Ok(())
    }

    /// Get checkpointer statistics
    async fn stats(&self) -> GraphResult<CheckpointerStats>;

    /// Perform cleanup operations (remove old checkpoints, etc.)
    async fn cleanup(&self) -> GraphResult<CleanupResult>;
}

/// Checkpointer statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointerStats {
    /// Total number of checkpoints stored
    pub total_checkpoints: usize,
    /// Number of unique threads
    pub unique_threads: usize,
    /// Total storage size in bytes
    pub storage_size_bytes: u64,
    /// Average checkpoint size in bytes
    pub avg_checkpoint_size_bytes: u64,
    /// Oldest checkpoint timestamp
    pub oldest_checkpoint: Option<DateTime<Utc>>,
    /// Newest checkpoint timestamp
    pub newest_checkpoint: Option<DateTime<Utc>>,
}

/// Result of cleanup operation
#[derive(Debug, Clone)]
pub struct CleanupResult {
    /// Number of checkpoints removed
    pub checkpoints_removed: usize,
    /// Storage space freed in bytes
    pub space_freed_bytes: u64,
    /// Duration of cleanup operation
    pub duration_ms: u64,
}

impl<S: GraphState> Checkpoint<S> {
    /// Create a new checkpoint
    pub fn new(id: String, state: S, step: u32) -> Self {
        Self {
            version: 1,
            id,
            timestamp: Utc::now(),
            state,
            step,
            next_nodes: Vec::new(),
            channel_values: HashMap::new(),
            channel_versions: HashMap::new(),
            node_history: Vec::new(),
            metadata: CheckpointMetadata::new(),
        }
    }

    /// Get checkpoint age
    pub fn age(&self) -> chrono::Duration {
        Utc::now() - self.timestamp
    }

    /// Check if checkpoint is older than duration
    pub fn is_older_than(&self, duration: chrono::Duration) -> bool {
        self.age() > duration
    }

    /// Add node execution to history
    pub fn add_node_execution(&mut self, execution: NodeExecution) {
        self.node_history.push(execution);
    }

    /// Get the latest node execution
    pub fn latest_node_execution(&self) -> Option<&NodeExecution> {
        self.node_history.last()
    }

    /// Get executions for a specific node
    pub fn node_executions(&self, node_name: &str) -> Vec<&NodeExecution> {
        self.node_history
            .iter()
            .filter(|exec| exec.node_name == node_name)
            .collect()
    }

    /// Calculate checkpoint size estimate
    pub fn size_estimate(&self) -> usize {
        // This is a rough estimate
        std::mem::size_of::<Self>() + 
        self.channel_values.len() * 100 + // Rough estimate for JSON values
        self.node_history.len() * std::mem::size_of::<NodeExecution>()
    }
}

impl CheckpointMetadata {
    /// Create new checkpoint metadata
    pub fn new() -> Self {
        Self {
            source: CheckpointSource::Loop,
            thread_id: None,
            user_id: None,
            parents: HashMap::new(),
            custom: HashMap::new(),
        }
    }

    /// Set the source
    pub fn with_source(mut self, source: CheckpointSource) -> Self {
        self.source = source;
        self
    }

    /// Set the thread ID
    pub fn with_thread_id(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    /// Set the user ID
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Add a parent checkpoint
    pub fn with_parent(mut self, namespace: impl Into<String>, checkpoint_id: impl Into<String>) -> Self {
        self.parents.insert(namespace.into(), checkpoint_id.into());
        self
    }

    /// Add custom metadata
    pub fn with_custom<T>(mut self, key: impl Into<String>, value: T) -> GraphResult<Self>
    where
        T: Serialize,
    {
        let json_value = serde_json::to_value(value)?;
        self.custom.insert(key.into(), json_value);
        Ok(self)
    }
}

impl Default for CheckpointMetadata {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeExecution {
    /// Create a new node execution record
    pub fn new(node_name: impl Into<String>, input_state: JsonValue) -> Self {
        Self {
            node_name: node_name.into(),
            started_at: Utc::now(),
            completed_at: None,
            status: ExecutionStatus::Pending,
            input_state,
            output_state: None,
            error: None,
            duration_ms: None,
        }
    }

    /// Mark execution as started
    pub fn start(&mut self) {
        self.status = ExecutionStatus::Running;
        self.started_at = Utc::now();
    }

    /// Mark execution as completed
    pub fn complete(&mut self, output_state: JsonValue) {
        self.status = ExecutionStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.output_state = Some(output_state);
        self.duration_ms = Some((Utc::now() - self.started_at).num_milliseconds() as u64);
    }

    /// Mark execution as failed
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = ExecutionStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error = Some(error.into());
        self.duration_ms = Some((Utc::now() - self.started_at).num_milliseconds() as u64);
    }

    pub fn interrupt(&mut self) {
        self.status = ExecutionStatus::Interrupted;
        self.completed_at = Some(Utc::now());
        self.duration_ms = Some((Utc::now() - self.started_at).num_milliseconds() as u64);
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.status, ExecutionStatus::Failed | ExecutionStatus::Interrupted)
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, ExecutionStatus::Running)
    }

    pub fn is_successful(&self) -> bool {
        matches!(self.status, ExecutionStatus::Completed)
    }

    /// Get execution duration
    pub fn duration(&self) -> Option<chrono::Duration> {
        self.completed_at.map(|end| end - self.started_at)
    }
}
