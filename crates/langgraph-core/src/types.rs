//! Core types and traits for LangGraph

use crate::errors::GraphResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Map as JsonMap;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::sync::Arc;
use uuid::Uuid;
use serde_json::Value as JsonValue;

/// Trait for types that can be used as graph state
pub trait GraphState:
    Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static
{
    /// Merge this state with another state
    /// Default implementation uses the other state (last writer wins)
    fn merge(&self, other: &Self) -> GraphResult<Self> {
        Ok(other.clone())
    }
}

impl<T> GraphState for T where
    T: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static
{
}

/// Partial state update returned by nodes
pub type StateUpdate = JsonMap<String, JsonValue>;

/// Node output containing optional state updates and execution command.
#[derive(Debug, Clone)]
pub struct NodeOutput<S: GraphState> {
    /// Partial state update to merge into the global state.
    pub update: StateUpdate,
    /// Optional execution command for routing/control flow.
    pub command: Option<Command<S>>,
}

impl<S: GraphState> NodeOutput<S> {
    /// Create node output with only update data.
    pub fn from_update(update: StateUpdate) -> Self {
        Self {
            update,
            command: None,
        }
    }

    /// Create node output with update and command.
    pub fn with_command(update: StateUpdate, command: Command<S>) -> Self {
        Self {
            update,
            command: Some(command),
        }
    }

    /// Create node output with only command.
    pub fn from_command(command: Command<S>) -> Self {
        Self {
            update: StateUpdate::new(),
            command: Some(command),
        }
    }
}

impl<S: GraphState> From<StateUpdate> for NodeOutput<S> {
    fn from(update: StateUpdate) -> Self {
        Self::from_update(update)
    }
}

impl<S: GraphState> From<Command<S>> for NodeOutput<S> {
    fn from(command: Command<S>) -> Self {
        Self::from_command(command)
    }
}

/// Node function type - async function that takes state and returns updated state
#[async_trait]
pub trait NodeFunction<S: GraphState>: Send + Sync {
    async fn call(&self, state: S, context: ExecutionContext) -> GraphResult<NodeOutput<S>>;
}

/// Blanket implementation for async functions
#[async_trait]
impl<S, F, Fut, O> NodeFunction<S> for F
where
    S: GraphState,
    F: Fn(S, ExecutionContext) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = GraphResult<O>> + Send,
    O: Into<NodeOutput<S>> + Send,
{
    async fn call(&self, state: S, context: ExecutionContext) -> GraphResult<NodeOutput<S>> {
        self(state, context).await.map(Into::into)
    }
}

/// Simple function without context
#[async_trait]
impl<S, Fut, O> NodeFunction<S> for fn(S) -> Fut
where
    S: GraphState,
    Fut: std::future::Future<Output = GraphResult<O>> + Send,
    O: Into<NodeOutput<S>> + Send,
{
    async fn call(&self, state: S, _context: ExecutionContext) -> GraphResult<NodeOutput<S>> {
        self(state).await.map(Into::into)
    }
}

/// Execution context passed to nodes during execution
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Unique execution ID
    pub execution_id: Uuid,
    /// Current step number
    pub step: u32,
    /// Node name being executed
    pub node_name: String,
    /// Graph name
    pub graph_name: String,
    /// User-provided configuration
    pub config: HashMap<String, JsonValue>,
    /// Runtime metadata
    pub metadata: HashMap<String, JsonValue>,
    /// Checkpoint ID if resuming
    pub checkpoint_id: Option<String>,
    /// Thread ID for conversation tracking
    pub thread_id: Option<String>,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new(node_name: impl Into<String>, graph_name: impl Into<String>) -> Self {
        Self {
            execution_id: Uuid::new_v4(),
            step: 0,
            node_name: node_name.into(),
            graph_name: graph_name.into(),
            config: HashMap::new(),
            metadata: HashMap::new(),
            checkpoint_id: None,
            thread_id: None,
        }
    }

    /// Get a configuration value
    pub fn get_config<T>(&self, key: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.config
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Set a configuration value
    pub fn set_config<T>(&mut self, key: impl Into<String>, value: T) -> GraphResult<()>
    where
        T: Serialize,
    {
        let json_value = serde_json::to_value(value)?;
        self.config.insert(key.into(), json_value);
        Ok(())
    }

    /// Get a metadata value
    pub fn get_metadata<T>(&self, key: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.metadata
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Set a metadata value
    pub fn set_metadata<T>(&mut self, key: impl Into<String>, value: T) -> GraphResult<()>
    where
        T: Serialize,
    {
        let json_value = serde_json::to_value(value)?;
        self.metadata.insert(key.into(), json_value);
        Ok(())
    }
}

/// Stream mode for graph execution output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamMode {
    /// Emit complete state after each step
    Values,
    /// Emit only state updates from each node
    Updates,
    /// Emit checkpoints when created
    Checkpoints,
    /// Emit task execution events
    Tasks,
    /// Emit debug information
    Debug,
    /// Emit custom stream data
    Custom,
}

impl Default for StreamMode {
    fn default() -> Self {
        StreamMode::Updates
    }
}

impl Display for StreamMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamMode::Values => write!(f, "values"),
            StreamMode::Updates => write!(f, "updates"),
            StreamMode::Checkpoints => write!(f, "checkpoints"),
            StreamMode::Tasks => write!(f, "tasks"),
            StreamMode::Debug => write!(f, "debug"),
            StreamMode::Custom => write!(f, "custom"),
        }
    }
}

/// Stream event emitted during graph execution
#[derive(Debug, Clone, Serialize)]
pub struct StreamEvent<S: GraphState> {
    /// Event type
    pub event_type: StreamEventType,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Execution step
    pub step: u32,
    /// Node name (if applicable)
    pub node: Option<String>,
    /// Event data
    pub data: StreamEventData<S>,
}

/// Types of stream events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamEventType {
    /// Node started execution
    NodeStart,
    /// Node completed execution
    NodeComplete,
    /// Node failed execution
    NodeError,
    /// State update occurred between each step
    StateUpdate,
    /// Checkpoint created
    Checkpoint,
    /// Graph execution started
    GraphStart,
    /// Graph execution completed
    GraphComplete,
    /// Graph execution failed
    GraphError,
    /// Custom event
    Custom(String),
}

/// Data payload for stream events
#[derive(Debug, Clone, Serialize)]
pub enum StreamEventData<S: GraphState> {
    /// State data
    State(S),
    /// State update (partial state)
    Update(JsonValue),
    /// Error information
    Error { message: String, code: u32 },
    /// Checkpoint information
    Checkpoint { id: String, step: u32 },
    /// Custom data
    Custom(JsonValue),
}

/// Details about an interrupted execution.
#[derive(Debug, Clone, Serialize)]
pub struct InterruptInfo<S: GraphState> {
    /// Last known state at interruption point.
    pub state: S,
    /// Step where interruption occurred.
    pub step: u32,
    /// Node associated with the interruption (if known).
    pub node: Option<String>,
    /// Human-readable interruption reason.
    pub reason: String,
    /// Nodes to execute when resuming.
    pub next_nodes: Vec<String>,
    /// Thread identifier associated with this execution.
    pub thread_id: Option<String>,
}

/// Result of an interrupt-aware graph invocation.
#[derive(Debug, Clone, Serialize)]
pub enum InvokeOutcome<S: GraphState> {
    /// Graph completed normally.
    Completed(S),
    /// Graph was interrupted and can be resumed.
    Interrupted(InterruptInfo<S>),
}

/// Configuration for graph execution
#[derive(Debug, Clone, Default)]
pub struct GraphConfig {
    /// Stream mode for output
    pub stream_mode: StreamMode,
    /// Whether to enable checkpointing
    pub checkpointing: bool,
    /// Maximum execution steps
    pub max_steps: Option<u32>,
    /// Node execution timeout
    pub timeout_ms: Option<u64>,
    /// Thread ID for conversation tracking
    pub thread_id: Option<String>,
    /// User configuration values
    pub config: HashMap<String, JsonValue>,
    /// Whether to enable debug mode
    pub debug: bool,
    /// Nodes to interrupt before execution
    pub interrupt_before: Vec<String>,
    /// Nodes to interrupt after execution
    pub interrupt_after: Vec<String>,
    /// Internal resume hint: nodes to run first when resuming.
    pub resume_next_nodes: Option<Vec<String>>,
}

impl GraphConfig {
    /// Create a new graph configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set stream mode
    pub fn with_stream_mode(mut self, mode: StreamMode) -> Self {
        self.stream_mode = mode;
        self
    }

    /// Enable checkpointing
    pub fn with_checkpointing(mut self, enabled: bool) -> Self {
        self.checkpointing = enabled;
        self
    }

    /// Set maximum steps
    pub fn with_max_steps(mut self, max_steps: u32) -> Self {
        self.max_steps = Some(max_steps);
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Set thread ID
    pub fn with_thread_id(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    /// Set configuration value
    pub fn with_config<T>(mut self, key: impl Into<String>, value: T) -> GraphResult<Self>
    where
        T: Serialize,
    {
        let json_value = serde_json::to_value(value)?;
        self.config.insert(key.into(), json_value);
        Ok(self)
    }

    /// Enable debug mode
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    /// Add interrupt before node
    pub fn with_interrupt_before(mut self, node: impl Into<String>) -> Self {
        self.interrupt_before.push(node.into());
        self
    }

    /// Add interrupt after node
    pub fn with_interrupt_after(mut self, node: impl Into<String>) -> Self {
        self.interrupt_after.push(node.into());
        self
    }

    /// Set explicit nodes to run first when resuming.
    pub fn with_resume_next_nodes(mut self, nodes: Vec<String>) -> Self {
        self.resume_next_nodes = Some(nodes);
        self
    }
}

/// Graph execution state snapshot
#[derive(Debug, Clone, Serialize)]
pub struct StateSnapshot<S: GraphState> {
    /// Snapshot ID
    pub id: String,
    /// Current state
    pub state: S,
    /// Current step
    pub step: u32,
    /// Next nodes to execute
    pub next_nodes: Vec<String>,
    /// Timestamp when snapshot was created
    pub timestamp: DateTime<Utc>,
    /// Execution metadata
    pub metadata: HashMap<String, JsonValue>,
}

/// GraphSend command for routing messages between nodes
#[derive(Debug, Clone, Serialize)]
pub struct GraphSend<S: GraphState> {
    /// Target node
    pub node: String,
    /// State to send
    pub state: S,
}

impl<S: GraphState> GraphSend<S> {
    /// Create a new send command
    pub fn new(node: impl Into<String>, state: S) -> Self {
        Self {
            node: node.into(),
            state,
        }
    }
}

/// Command for controlling graph execution flow
#[derive(Debug, Clone, Serialize)]
pub enum Command<S: GraphState> {
    /// Continue execution
    Continue,
    /// Go to specific node
    Goto(String),
    /// GraphSend to multiple nodes
    Send(Vec<GraphSend<S>>),
    /// Interrupt execution
    Interrupt(String),
    /// Resume execution from checkpoint
    Resume(String),
}

/// Node specification in the graph
pub struct NodeSpec<S: GraphState> {
    /// Node name
    pub name: String,
    /// Node function
    pub function: Arc<dyn NodeFunction<S>>,
    /// Node metadata
    pub metadata: HashMap<String, JsonValue>,
    /// Retry policy
    pub retry_policy: Option<RetryPolicy>,
    /// Cache policy
    pub cache_policy: Option<CachePolicy>,
}

impl<S: GraphState> NodeSpec<S> {
    /// Create a new node specification
    pub fn new(name: impl Into<String>, function: impl NodeFunction<S> + 'static) -> Self {
        Self {
            name: name.into(),
            function: Arc::new(function),
            metadata: HashMap::new(),
            retry_policy: None,
            cache_policy: None,
        }
    }

    /// Set metadata
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Serialize,
    ) -> GraphResult<Self> {
        let json_value = serde_json::to_value(value)?;
        self.metadata.insert(key.into(), json_value);
        Ok(self)
    }

    /// Set retry policy
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = Some(policy);
        self
    }

    /// Set cache policy
    pub fn with_cache_policy(mut self, policy: CachePolicy) -> Self {
        self.cache_policy = Some(policy);
        self
    }
}

/// Retry policy for node execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Base delay between retries in milliseconds
    pub base_delay_ms: u64,
    /// Maximum delay between retries in milliseconds
    pub max_delay_ms: u64,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 10000,
            backoff_multiplier: 2.0,
        }
    }
}

/// Cache policy for node execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePolicy {
    /// Whether to enable caching
    pub enabled: bool,
    /// Cache TTL in seconds
    pub ttl_seconds: Option<u64>,
    /// Cache key prefix
    pub key_prefix: Option<String>,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            ttl_seconds: None,
            key_prefix: None,
        }
    }
}

/// Edge specification in the graph
#[derive(Clone)]
pub struct EdgeSpec {
    /// Source node
    pub from: String,
    /// Target node
    pub to: String,
    /// Edge condition (for conditional edges)
    pub condition: Option<Arc<dyn EdgeCondition + Send + Sync>>,
}

/// Trait for edge conditions
pub trait EdgeCondition: Send + Sync {
    fn evaluate(&self, state: &JsonValue) -> GraphResult<bool>;
}

/// Branch specification for conditional routing
pub struct BranchSpec<S: GraphState> {
    /// Condition function
    pub condition: Arc<dyn Fn(&S) -> GraphResult<String> + Send + Sync>,
    /// Possible target nodes
    pub targets: Vec<String>,
}

impl<S: GraphState> BranchSpec<S> {
    /// Create a new branch specification
    pub fn new<F>(condition: F, targets: Vec<String>) -> Self
    where
        F: Fn(&S) -> GraphResult<String> + Send + Sync + 'static,
    {
        Self {
            condition: Arc::new(condition),
            targets,
        }
    }
}

/// Graph execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStats {
    /// Total execution time in milliseconds
    pub total_time_ms: u64,
    /// Number of steps executed
    pub steps: u32,
    /// Number of nodes executed
    pub nodes_executed: u32,
    /// Number of errors encountered
    pub errors: u32,
    /// Number of retries performed
    pub retries: u32,
    /// Memory usage statistics
    pub memory_stats: MemoryStats,
}

/// Memory usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Peak memory usage in bytes
    pub peak_memory_bytes: u64,
    /// Current memory usage in bytes
    pub current_memory_bytes: u64,
    /// Number of allocations
    pub allocations: u64,
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self {
            peak_memory_bytes: 0,
            current_memory_bytes: 0,
            allocations: 0,
        }
    }
}
