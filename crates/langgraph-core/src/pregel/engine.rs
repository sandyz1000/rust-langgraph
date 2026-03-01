//! Core Pregel execution engine implementation
//!
//! What is BSP (Bulk Synchronous Parallel)?
//! BSP is a parallel computing model that consists of three key phases that repeat in cycles called "supersteps":
//!
//! ### 🔄 The Three BSP Phases:
//! - *🏃 COMPUTATION Phase*: All processors/nodes execute tasks in parallel using only data from the previous superstep
//! - *📡 COMMUNICATION Phase*: All processors send messages/updates to other processors
//! - *⚡ SYNCHRONIZATION Phase*: A global synchronization barrier where all processors wait for others to complete
//! before proceeding to the next superstep
//!
//! ### 💡 Key BSP Principles in LangGraph:
//! - *Parallel Execution*: All nodes in a superstep run simultaneously without interfering with each other
//! - *Immutable Input*: During execution, nodes only see the state from the previous step - no mid-step state changes
//! - *Atomic Updates*: All state changes are collected first, then applied together after all nodes complete
//! - *Synchronization Barrier*: The system waits for all nodes to finish before moving to the next step
//!
//! ### 🔍 In the Code:
//! ```text
//! // Phase 1: COMPUTATION - Execute all tasks in parallel
//! let task_results = self.scheduler.execute_tasks(tasks).await?;
//!
//! // Phase 2: COMMUNICATION - Collect all outputs
//! let mut state_updates = Vec::new();
//! for result in task_results { /* collect state changes */ }
//!
//! // Phase 3: SYNCHRONIZATION - Apply all updates atomically
//! let final_state = self.merge_states(&current_state, &state_updates)?;
//! ```
//!
//! 🎯 Why BSP for Graph Execution?
//! - *Deterministic*: Same input always produces same output regardless of execution timing
//! - *Scalable*: Nodes can run on different processors/threads safely
//! - *Fault Tolerant*: Each superstep is atomic - if something fails, you can restart from the last completed step
//!
use crate::channels::{BinaryOpReducer, ChannelSpec, ChannelType};
use crate::constants::{END, MAX_RECURSION_DEPTH};
use crate::errors::{GraphResult, LangGraphError};
use crate::graph::CompiledGraph;
use crate::pregel::{PregelTask, TaskResult, TaskScheduler, TaskStatus};
use crate::types::{
    CachePolicy, ExecutionContext, ExecutionStats, GraphConfig, GraphState, MemoryStats,
    RetryPolicy, StateSnapshot, StateUpdate, StreamEvent, StreamEventData, StreamEventType,
    StreamMode,
};
use chrono::Utc;
use dashmap::DashMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_stream::{wrappers::ReceiverStream, Stream};
use uuid::Uuid;

/// Pregel execution engine for graph computation following BSP (Bulk Synchronous Parallel) model
pub struct PregelEngine<S: GraphState> {
    /// Task scheduler for managing concurrent execution
    scheduler: TaskScheduler<S>,
    /// Active graph executions
    executions: Arc<DashMap<String, ExecutionState<S>>>,
    /// Graph snapshots for debugging/monitoring
    snapshots: Arc<DashMap<String, Vec<StateSnapshot<S>>>>,
    /// Node-level cache for deterministic pure-node updates
    node_cache: Arc<DashMap<String, StateUpdate>>,
    /// Execution statistics
    stats: Arc<RwLock<ExecutionStats>>,
}
/// Pregel loop state for a single execution (similar to Python PregelLoop)
struct ExecutionState<S: GraphState> {
    /// Current graph state (represents channel values)
    current_state: Arc<RwLock<S>>,
    /// Current superstep counter
    step: AtomicU32,
    /// Execution context containing metadata
    context: ExecutionContext,
    /// Graph configuration
    config: GraphConfig,
    /// Event broadcaster for streaming
    event_sender: broadcast::Sender<StreamEvent<S>>,
    /// Nodes executed in the previous superstep
    previous_nodes: Arc<RwLock<Vec<String>>>,
    /// Explicit next nodes requested by command routing
    next_nodes_override: Arc<RwLock<Option<Vec<String>>>>,
    /// Last computed next nodes for checkpoint snapshots
    next_nodes_for_checkpoint: Arc<RwLock<Vec<String>>>,
    /// Status of current execution
    status: Arc<RwLock<ExecutionStatus>>,
}

/// Execution status following Pregel model
#[derive(Debug, Clone, PartialEq)]
enum ExecutionStatus {
    /// Waiting for input
    Input,
    /// Tasks are pending execution  
    Pending,
    /// Execution completed successfully
    Done,
    /// Interrupted before task execution
    InterruptBefore,
    /// Interrupted after task execution
    InterruptAfter,
    /// Reached maximum steps without completion
    OutOfSteps,
}

impl<S: GraphState> PregelEngine<S> {
    /// Create a new Pregel engine
    pub fn new() -> Self {
        Self {
            scheduler: TaskScheduler::new(),
            executions: Arc::new(DashMap::new()),
            snapshots: Arc::new(DashMap::new()),
            node_cache: Arc::new(DashMap::new()),
            stats: Arc::new(RwLock::new(ExecutionStats {
                total_time_ms: 0,
                steps: 0,
                nodes_executed: 0,
                errors: 0,
                retries: 0,
                memory_stats: MemoryStats::default(),
            })),
        }
    }

    /// Execute a graph with the given input and configuration
    pub async fn execute(
        &self,
        graph: &CompiledGraph<S>,
        input: S,
        config: GraphConfig,
    ) -> GraphResult<impl Stream<Item = StreamEvent<S>> + '_> {
        let execution_id = Uuid::new_v4().to_string();
        let thread_id = config
            .thread_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        // Create execution context
        let mut context = ExecutionContext::new("__start__", "graph");
        context.execution_id = Uuid::parse_str(&execution_id).unwrap();
        context.thread_id = Some(thread_id.clone());
        context.config = config.config.clone();

        // Create event channel
        let (event_sender, _) = broadcast::channel(1000);
        let event_receiver = event_sender.subscribe();

        // Create execution state
        let execution_state = ExecutionState {
            current_state: Arc::new(RwLock::new(input.clone())),
            step: AtomicU32::new(0),
            context,
            config: config.clone(),
            event_sender: event_sender.clone(),
            previous_nodes: Arc::new(RwLock::new(Vec::new())),
            next_nodes_override: Arc::new(RwLock::new(config.resume_next_nodes.clone())),
            next_nodes_for_checkpoint: Arc::new(RwLock::new(graph.entry_points.clone())),
            status: Arc::new(RwLock::new(ExecutionStatus::Input)),
        };

        // Send graph start event
        let start_event = StreamEvent {
            event_type: StreamEventType::GraphStart,
            timestamp: Utc::now(),
            step: 0,
            node: None,
            data: StreamEventData::State(input.clone()),
        };
        self.emit_event(&execution_state, start_event);

        // Store execution state
        self.executions
            .insert(execution_id.clone(), execution_state);

        // Create initial snapshot
        let initial_snapshot = StateSnapshot {
            id: Uuid::new_v4().to_string(),
            state: input.clone(),
            step: 0,
            next_nodes: graph.entry_points.clone(),
            timestamp: Utc::now(),
            metadata: HashMap::from([
                ("thread_id".to_string(), serde_json::json!(thread_id)),
                (
                    "config".to_string(),
                    serde_json::json!(config.config.clone()),
                ),
            ]),
        };

        // Store snapshot
        self.snapshots
            .entry(thread_id.clone())
            .or_insert_with(Vec::new)
            .push(initial_snapshot);

        // Start execution
        self.execute_graph(graph, &execution_id).await?;

        // Convert broadcast receiver to stream
        let stream = ReceiverStream::new(self.convert_broadcast_to_receiver(event_receiver).await);

        Ok(stream)
    }

    /// Helper to convert broadcast receiver to mpsc receiver
    async fn convert_broadcast_to_receiver(
        &self,
        mut broadcast_rx: broadcast::Receiver<StreamEvent<S>>,
    ) -> mpsc::Receiver<StreamEvent<S>> {
        let (tx, rx) = mpsc::channel(1000);

        tokio::spawn(async move {
            while let Ok(event) = broadcast_rx.recv().await {
                if tx.send(event).await.is_err() {
                    break;
                }
            }
        });

        rx
    }

    /// Execute the complete Pregel algorithm following BSP (Bulk Synchronous Parallel) model
    /// Based on LangGraph's PregelLoop implementation
    async fn execute_graph(&self, graph: &CompiledGraph<S>, execution_id: &str) -> GraphResult<()> {
        let execution = self
            .executions
            .get(execution_id)
            .ok_or_else(|| LangGraphError::runtime("Execution not found"))?;

        // Initialize Pregel loop state
        {
            let mut status = execution.status.write().await;
            *status = ExecutionStatus::Pending;
        }

        // Main Pregel Loop - follows while loop.tick() pattern
        while self.pregel_tick(graph, execution_id).await? {
            // Continue until completion or interruption

            // Check recursion depth
            let current_step = execution.step.load(Ordering::Relaxed);
            let step_limit = execution
                .config
                .max_steps
                .unwrap_or(MAX_RECURSION_DEPTH as u32);
            if current_step >= step_limit {
                {
                    let mut status = execution.status.write().await;
                    *status = ExecutionStatus::OutOfSteps;
                }
                return Err(LangGraphError::recursion_limit(step_limit as usize));
            }

            // Create checkpoint if configured
            if execution.config.checkpointing {
                self.create_checkpoint(&execution, current_step).await?;
            }
        }

        // Handle final status
        let final_status = {
            let status = execution.status.read().await;
            status.clone()
        };

        match final_status {
            ExecutionStatus::Done => {
                // Send completion event
                let final_state = {
                    let state = execution.current_state.read().await;
                    state.clone()
                };

                let complete_event = StreamEvent {
                    event_type: StreamEventType::GraphComplete,
                    timestamp: Utc::now(),
                    step: execution.step.load(Ordering::Relaxed),
                    node: None,
                    data: StreamEventData::State(final_state),
                };
                self.emit_event(&execution, complete_event);
            }
            ExecutionStatus::InterruptBefore | ExecutionStatus::InterruptAfter => {
                // Interruption was handled during execution
            }
            ExecutionStatus::OutOfSteps => {
                return Err(LangGraphError::recursion_limit(MAX_RECURSION_DEPTH));
            }
            _ => {
                return Err(LangGraphError::runtime(&format!(
                    "Unexpected execution status: {:?}",
                    final_status
                )));
            }
        }

        // Clean up execution
        drop(execution);
        self.executions.remove(execution_id);
        Ok(())
    }

    /// Execute a single Pregel tick (superstep) - core BSP algorithm
    /// Returns true if more iterations are needed, false when done
    async fn pregel_tick(&self, graph: &CompiledGraph<S>, execution_id: &str) -> GraphResult<bool> {
        let execution = self
            .executions
            .get(execution_id)
            .ok_or_else(|| LangGraphError::runtime("Execution not found"))?;

        // Increment step counter (like self.step)
        let current_step = execution.step.fetch_add(1, Ordering::Relaxed) + 1;

        // Phase 1: PREPARE NEXT TASKS
        // This is equivalent to prepare_next_tasks()
        let current_nodes = self
            .prepare_next_tasks(graph, &execution, current_step)
            .await?;

        // If no tasks to execute, we're done
        if current_nodes.is_empty() {
            let mut status = execution.status.write().await;
            *status = ExecutionStatus::Done;
            return Ok(false);
        }

        // Phase 2: CHECK INTERRUPTS BEFORE
        if self.should_interrupt_before(&execution.config, &current_nodes) {
            self.send_interrupt_event(&execution, &current_nodes, "before")
                .await?;
            let mut status = execution.status.write().await;
            *status = ExecutionStatus::InterruptBefore;
            return Ok(false);
        }

        // Phase 3: EXECUTE STEP (all nodes in parallel - BSP)
        let task_results = self
            .execute_step_bsp(graph, execution_id, &current_nodes)
            .await?;

        // Phase 4: CHECK INTERRUPTS AFTER
        if self.should_interrupt_after(&execution.config, &current_nodes) {
            self.send_interrupt_event(&execution, &current_nodes, "after")
                .await?;
            let mut status = execution.status.write().await;
            *status = ExecutionStatus::InterruptAfter;
            return Ok(false);
        }

        // Phase 5: AFTER TICK - Apply channel updates atomically
        // This is equivalent to loop.after_tick()
        let has_more_work = self.after_tick(graph, &execution, &task_results).await?;

        if !has_more_work {
            let mut status = execution.status.write().await;
            *status = ExecutionStatus::Done;
        }

        Ok(has_more_work)
    }

    /// Prepare tasks for the next superstep (like prepare_next_tasks)
    async fn prepare_next_tasks(
        &self,
        graph: &CompiledGraph<S>,
        execution: &ExecutionState<S>,
        step: u32,
    ) -> GraphResult<Vec<String>> {
        // Command-driven routing takes precedence over static edge/branch routing
        {
            let mut override_nodes = execution.next_nodes_override.write().await;
            if let Some(nodes) = override_nodes.take() {
                return Ok(nodes);
            }
        }

        // For the first step, use entry points
        if step == 1 {
            return Ok(graph.entry_points.clone());
        }

        let previous_nodes = {
            let nodes = execution.previous_nodes.read().await;
            nodes.clone()
        };

        if previous_nodes.is_empty() {
            return Ok(Vec::new());
        }

        let current_state = {
            let state = execution.current_state.read().await;
            state.clone()
        };

        let mut next_nodes = HashSet::new();
        for node_name in &previous_nodes {
            let successors = self
                .get_next_nodes(graph, node_name, &current_state)
                .await?;
            for successor in successors {
                if successor != END && !graph.finish_points.contains(&successor) {
                    next_nodes.insert(successor);
                }
            }
        }

        let mut ordered = next_nodes.into_iter().collect::<Vec<_>>();
        ordered.sort();
        Ok(ordered)
    }

    /// Execute BSP step - all tasks run in parallel, results collected before channel updates
    /// This implements the core Pregel/BSP execution pattern
    async fn execute_step_bsp(
        &self,
        graph: &CompiledGraph<S>,
        execution_id: &str,
        current_nodes: &[String],
    ) -> GraphResult<Vec<TaskResult>> {
        let execution = self
            .executions
            .get(execution_id)
            .ok_or_else(|| LangGraphError::runtime("Execution not found"))?;

        let current_step = execution.step.load(Ordering::Relaxed);
        let mut tasks = Vec::new();
        let mut cached_results = Vec::new();
        let mut cache_keys_by_task_id: HashMap<String, String> = HashMap::new();

        // PREPARE TASKS - Create executable tasks for each node
        for node_name in current_nodes {
            if let Some(node_spec) = graph.nodes.get(node_name) {
                // Read current state from channels (immutable during step execution)
                let node_input = {
                    let state = execution.current_state.read().await;
                    state.clone()
                };

                // Create execution context
                let mut task_context = execution.context.clone();
                task_context.node_name = node_name.clone();
                task_context.step = current_step;

                let max_steps = execution
                    .config
                    .max_steps
                    .unwrap_or(MAX_RECURSION_DEPTH as u32);
                let remaining_steps = max_steps.saturating_sub(current_step);
                task_context
                    .set_metadata("remaining_steps", remaining_steps)
                    .ok();
                task_context
                    .set_metadata("is_last_step", current_step >= max_steps)
                    .ok();

                if let Some(RetryPolicy {
                    max_retries,
                    base_delay_ms,
                    ..
                }) = &node_spec.retry_policy
                {
                    task_context.set_config("max_retries", *max_retries).ok();
                    task_context
                        .set_config("retry_delay_ms", *base_delay_ms)
                        .ok();
                }

                if let Some(timeout_ms) = execution.config.timeout_ms {
                    task_context.set_config("timeout_ms", timeout_ms).ok();
                }

                // Generate deterministic task ID
                let task_id = format!("{}:{}:{}", execution_id, current_step, node_name);

                if let Some(CachePolicy { enabled: true, .. }) = node_spec.cache_policy.as_ref() {
                    let cache_input = serde_json::to_string(&node_input)?;
                    let cache_key = format!("{}:{}", node_name, cache_input);
                    if let Some(cached) = self.node_cache.get(&cache_key) {
                        cached_results.push(TaskResult::success(
                            task_id.clone(),
                            node_name.clone(),
                            cached.clone(),
                            0,
                        ));
                        continue;
                    }
                    cache_keys_by_task_id.insert(task_id.clone(), cache_key);
                }

                let task = PregelTask::new(
                    task_id.clone(),
                    node_name.clone(),
                    node_input,
                    task_context,
                    node_spec.function.clone(),
                );

                tasks.push(task);

                // Emit task start event
                let start_event = StreamEvent {
                    event_type: StreamEventType::NodeStart,
                    timestamp: Utc::now(),
                    step: current_step,
                    node: Some(node_name.clone()),
                    data: StreamEventData::Custom(serde_json::json!({
                        "task_id": task_id
                    })),
                };
                self.emit_event(&execution, start_event);
            }
        }

        // EXECUTE ALL TASKS IN PARALLEL (Bulk Synchronous Parallel)
        let mut task_results = self.scheduler.execute_tasks(tasks).await?;
        task_results.extend(cached_results);

        for result in &task_results {
            if let (Some(update), Some(cache_key)) = (
                result.output_update.as_ref(),
                cache_keys_by_task_id.get(&result.task_id),
            ) {
                self.node_cache.insert(cache_key.clone(), update.clone());
            }
        }

        // Process results and emit events
        for result in &task_results {
            match result.status {
                TaskStatus::Completed => {
                    let complete_event = StreamEvent {
                        event_type: StreamEventType::NodeComplete,
                        timestamp: Utc::now(),
                        step: current_step,
                        node: Some(result.node_name.clone()),
                        data: StreamEventData::Custom(serde_json::json!({
                            "duration_ms": result.duration_ms,
                            "has_writes": result.output_update.is_some()
                        })),
                    };
                    self.emit_event(&execution, complete_event);

                    if matches!(
                        execution.config.stream_mode,
                        StreamMode::Updates | StreamMode::Debug | StreamMode::Custom
                    ) {
                        if let Some(ref update) = result.output_update {
                            let update_event = StreamEvent {
                                event_type: StreamEventType::StateUpdate,
                                timestamp: Utc::now(),
                                step: current_step,
                                node: Some(result.node_name.clone()),
                                data: StreamEventData::Update(serde_json::to_value(update)?),
                            };
                            self.emit_event(&execution, update_event);
                        }
                    }
                }
                TaskStatus::Failed => {
                    let error_event = StreamEvent {
                        event_type: StreamEventType::NodeError,
                        timestamp: Utc::now(),
                        step: current_step,
                        node: Some(result.node_name.clone()),
                        data: StreamEventData::Error {
                            message: result
                                .error
                                .clone()
                                .unwrap_or_else(|| "Unknown error".to_string()),
                            code: 1001,
                        },
                    };
                    self.emit_event(&execution, error_event);

                    return Err(LangGraphError::node_execution(
                        result.node_name.clone(),
                        "Node execution failed",
                    ));
                }
                _ => {}
            }
        }

        Ok(task_results)
    }

    /// Apply channel updates after task execution (like loop.after_tick())
    /// This is where the "synchronization barrier" happens in BSP
    async fn after_tick(
        &self,
        graph: &CompiledGraph<S>,
        execution: &ExecutionState<S>,
        task_results: &[TaskResult],
    ) -> GraphResult<bool> {
        let current_step = execution.step.load(Ordering::Relaxed);

        // Collect all state writes from completed tasks
        let mut state_updates = Vec::new();
        let mut completed_nodes = Vec::new();
        let mut command_next_nodes = HashSet::new();
        let mut interrupt_message = None;
        for result in task_results {
            if let (TaskStatus::Completed, Some(ref update)) =
                (&result.status, &result.output_update)
            {
                state_updates.push((result.node_name.clone(), update.clone()));
                completed_nodes.push(result.node_name.clone());
            }

            if let Some(ref targets) = result.routing_targets {
                for target in targets {
                    if target == END {
                        continue;
                    }

                    if !graph.nodes.contains_key(target) {
                        return Err(LangGraphError::node_not_found(target));
                    }

                    command_next_nodes.insert(target.clone());
                }
            }

            if interrupt_message.is_none() {
                interrupt_message = result.interrupt_message.clone();
            }
        }

        if state_updates.is_empty() {
            if let Some(message) = interrupt_message {
                self.send_interrupt_event(&execution, &completed_nodes, &message)
                    .await?;
                let mut status = execution.status.write().await;
                *status = ExecutionStatus::InterruptAfter;
                return Ok(false);
            }

            if !command_next_nodes.is_empty() {
                let mut explicit_next = command_next_nodes.into_iter().collect::<Vec<_>>();
                explicit_next.sort();

                {
                    let mut checkpoint_next = execution.next_nodes_for_checkpoint.write().await;
                    *checkpoint_next = explicit_next.clone();
                }

                let mut override_nodes = execution.next_nodes_override.write().await;
                *override_nodes = Some(explicit_next);
                return Ok(true);
            }

            // No writes and no command routing means no more work
            return Ok(false);
        }

        // Apply all writes atomically to channels
        if !state_updates.is_empty() {
            let final_state = {
                let current_state = execution.current_state.read().await;
                self.merge_states(&current_state, &state_updates, Some(&graph.channels))?
            };

            // Update execution state (channel values)
            {
                let mut state = execution.current_state.write().await;
                *state = final_state.clone();
            }

            // Track completed nodes for next step scheduling
            {
                let mut previous_nodes = execution.previous_nodes.write().await;
                *previous_nodes = completed_nodes.clone();
            }

            // Emit state update event
            let update_event = StreamEvent {
                event_type: StreamEventType::StateUpdate,
                timestamp: Utc::now(),
                step: current_step,
                node: None, // Multiple nodes contributed
                data: StreamEventData::State(final_state.clone()),
            };
            self.emit_event(&execution, update_event);

            // If a node requested an interrupt command, stop execution after applying updates
            if let Some(message) = interrupt_message {
                self.send_interrupt_event(&execution, &completed_nodes, &message)
                    .await?;
                let mut status = execution.status.write().await;
                *status = ExecutionStatus::InterruptAfter;
                return Ok(false);
            }

            // Command-driven routing overrides static edge/branch routing for next step
            if !command_next_nodes.is_empty() {
                let mut explicit_next = command_next_nodes.into_iter().collect::<Vec<_>>();
                explicit_next.sort();

                {
                    let mut checkpoint_next = execution.next_nodes_for_checkpoint.write().await;
                    *checkpoint_next = explicit_next.clone();
                }

                let mut override_nodes = execution.next_nodes_override.write().await;
                *override_nodes = Some(explicit_next);
                return Ok(true);
            }

            // Determine if there's more work by checking for next nodes
            let mut has_next_nodes = false;
            let mut aggregate_next_nodes = HashSet::new();
            for node_name in &completed_nodes {
                let next_nodes = self.get_next_nodes(graph, node_name, &final_state).await?;
                if !next_nodes.is_empty() {
                    has_next_nodes = true;
                    aggregate_next_nodes.extend(next_nodes);
                }
            }

            if has_next_nodes {
                let mut ordered_next = aggregate_next_nodes.into_iter().collect::<Vec<_>>();
                ordered_next.sort();
                let mut checkpoint_next = execution.next_nodes_for_checkpoint.write().await;
                *checkpoint_next = ordered_next;
            }
            return Ok(has_next_nodes);
        }

        Ok(false)
    }

    /// Merge multiple state updates into a single state for workflow management
    /// Uses a sophisticated merge strategy suitable for workflow orchestration
    fn merge_states(
        &self,
        base_state: &S,
        updates: &Vec<(String, StateUpdate)>,
        channels: Option<&HashMap<String, ChannelSpec>>,
    ) -> GraphResult<S> {
        if updates.is_empty() {
            return Ok(base_state.clone());
        }

        let mut merged_state = base_state.clone();

        // Strategy 1: Node-priority based merging
        // Sort updates by node priority/execution order to ensure deterministic merging
        let mut prioritized_updates = updates.clone();
        prioritized_updates.sort_by(|a, b| {
            self.get_node_priority(&a.0)
                .cmp(&self.get_node_priority(&b.0))
                .then_with(|| a.0.cmp(&b.0))
        });

        for (node_name, update) in prioritized_updates {
            merged_state = self.merge_single_state(&merged_state, &update, &node_name, channels)?;
        }

        Ok(merged_state)
    }

    /// Merge a single state update with sophisticated conflict resolution
    fn merge_single_state(
        &self,
        base: &S,
        update: &StateUpdate,
        _node_name: &str,
        channels: Option<&HashMap<String, ChannelSpec>>,
    ) -> GraphResult<S> {
        let mut base_json = serde_json::to_value(base)?;

        if let serde_json::Value::Object(base_map) = &mut base_json {
            for (key, value) in update {
                if let Some(spec) = channels.and_then(|channel_map| channel_map.get(key)) {
                    let merged_value = Self::merge_channel_value(spec, base_map.get(key), value)?;
                    base_map.insert(key.clone(), merged_value);
                } else {
                    base_map.insert(key.clone(), value.clone());
                }
            }
        } else {
            return Err(LangGraphError::invalid_update(
                "GraphState must serialize to JSON object for StateUpdate merging",
            ));
        }

        serde_json::from_value(base_json).map_err(LangGraphError::from)
    }

    fn merge_channel_value(
        spec: &ChannelSpec,
        current: Option<&serde_json::Value>,
        incoming: &serde_json::Value,
    ) -> GraphResult<serde_json::Value> {
        match &spec.channel_type {
            ChannelType::LastValue | ChannelType::Ephemeral => Ok(incoming.clone()),
            ChannelType::Accumulator => {
                let mut out = match current {
                    Some(serde_json::Value::Array(existing)) => existing.clone(),
                    Some(existing) => vec![existing.clone()],
                    None => Vec::new(),
                };

                match incoming {
                    serde_json::Value::Array(values) => out.extend(values.clone()),
                    value => out.push(value.clone()),
                }

                Ok(serde_json::Value::Array(out))
            }
            ChannelType::BinaryOp { reducer } => match reducer {
                BinaryOpReducer::Add => {
                    if incoming.is_i64() {
                        let current_num = current.and_then(|v| v.as_i64()).unwrap_or(0);
                        let incoming_num = incoming.as_i64().ok_or_else(|| {
                            LangGraphError::invalid_update(
                                "BinaryOp(add) requires numeric update values",
                            )
                        })?;
                        let sum = current_num.saturating_add(incoming_num);
                        return Ok(serde_json::Value::Number(serde_json::Number::from(sum)));
                    }

                    if incoming.is_u64() {
                        let current_num = current.and_then(|v| v.as_u64()).unwrap_or(0);
                        let incoming_num = incoming.as_u64().ok_or_else(|| {
                            LangGraphError::invalid_update(
                                "BinaryOp(add) requires numeric update values",
                            )
                        })?;
                        let sum = current_num.saturating_add(incoming_num);
                        return Ok(serde_json::Value::Number(serde_json::Number::from(sum)));
                    }

                    let current_num = current.and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let incoming_num = incoming.as_f64().ok_or_else(|| {
                        LangGraphError::invalid_update(
                            "BinaryOp(add) requires numeric update values",
                        )
                    })?;

                    let sum = current_num + incoming_num;
                    if let Some(number) = serde_json::Number::from_f64(sum) {
                        Ok(serde_json::Value::Number(number))
                    } else {
                        Err(LangGraphError::invalid_update(
                            "BinaryOp(add) produced a non-finite number",
                        ))
                    }
                }
                BinaryOpReducer::Max => {
                    if incoming.is_i64() {
                        let current_num = current.and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
                        let incoming_num = incoming.as_i64().ok_or_else(|| {
                            LangGraphError::invalid_update(
                                "BinaryOp(max) requires numeric update values",
                            )
                        })?;
                        return Ok(serde_json::Value::Number(serde_json::Number::from(
                            current_num.max(incoming_num),
                        )));
                    }

                    if incoming.is_u64() {
                        let current_num = current.and_then(|v| v.as_u64()).unwrap_or(0);
                        let incoming_num = incoming.as_u64().ok_or_else(|| {
                            LangGraphError::invalid_update(
                                "BinaryOp(max) requires numeric update values",
                            )
                        })?;
                        return Ok(serde_json::Value::Number(serde_json::Number::from(
                            current_num.max(incoming_num),
                        )));
                    }

                    let current_num = current
                        .and_then(|v| v.as_f64())
                        .unwrap_or(f64::NEG_INFINITY);
                    let incoming_num = incoming.as_f64().ok_or_else(|| {
                        LangGraphError::invalid_update(
                            "BinaryOp(max) requires numeric update values",
                        )
                    })?;
                    let out = current_num.max(incoming_num);
                    if let Some(number) = serde_json::Number::from_f64(out) {
                        Ok(serde_json::Value::Number(number))
                    } else {
                        Err(LangGraphError::invalid_update(
                            "BinaryOp(max) produced a non-finite number",
                        ))
                    }
                }
                BinaryOpReducer::Min => {
                    if incoming.is_i64() {
                        let current_num = current.and_then(|v| v.as_i64()).unwrap_or(i64::MAX);
                        let incoming_num = incoming.as_i64().ok_or_else(|| {
                            LangGraphError::invalid_update(
                                "BinaryOp(min) requires numeric update values",
                            )
                        })?;
                        return Ok(serde_json::Value::Number(serde_json::Number::from(
                            current_num.min(incoming_num),
                        )));
                    }

                    if incoming.is_u64() {
                        let current_num = current.and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
                        let incoming_num = incoming.as_u64().ok_or_else(|| {
                            LangGraphError::invalid_update(
                                "BinaryOp(min) requires numeric update values",
                            )
                        })?;
                        return Ok(serde_json::Value::Number(serde_json::Number::from(
                            current_num.min(incoming_num),
                        )));
                    }

                    let current_num = current.and_then(|v| v.as_f64()).unwrap_or(f64::INFINITY);
                    let incoming_num = incoming.as_f64().ok_or_else(|| {
                        LangGraphError::invalid_update(
                            "BinaryOp(min) requires numeric update values",
                        )
                    })?;
                    let out = current_num.min(incoming_num);
                    if let Some(number) = serde_json::Number::from_f64(out) {
                        Ok(serde_json::Value::Number(number))
                    } else {
                        Err(LangGraphError::invalid_update(
                            "BinaryOp(min) produced a non-finite number",
                        ))
                    }
                }
                BinaryOpReducer::Concat => match incoming {
                    serde_json::Value::String(s) => {
                        let mut out = current
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        out.push_str(s);
                        Ok(serde_json::Value::String(out))
                    }
                    serde_json::Value::Array(values) => {
                        let mut out = match current {
                            Some(serde_json::Value::Array(existing)) => existing.clone(),
                            Some(existing) => vec![existing.clone()],
                            None => Vec::new(),
                        };
                        out.extend(values.clone());
                        Ok(serde_json::Value::Array(out))
                    }
                    _ => Err(LangGraphError::invalid_update(
                        "BinaryOp(concat) requires string or array update values",
                    )),
                },
            },
        }
    }

    /// Get node priority for deterministic merging
    /// Higher priority nodes override lower priority ones in conflicts
    fn get_node_priority(&self, node_name: &str) -> u32 {
        match node_name {
            // System nodes have highest priority
            name if name.starts_with("__") => 1000,
            // Decision/routing nodes have high priority
            name if name.contains("decision") || name.contains("route") => 800,
            // Processing nodes have medium priority
            name if name.contains("process") || name.contains("transform") => 600,
            // Data/input nodes have lower priority
            name if name.contains("input") || name.contains("data") => 400,
            // Default priority
            _ => 500,
        }
    }

    /// Get next nodes based on edges and conditional logic
    async fn get_next_nodes(
        &self,
        graph: &CompiledGraph<S>,
        from_node: &str,
        state: &S,
    ) -> GraphResult<Vec<String>> {
        let mut next_nodes = Vec::new();

        // Check direct edges
        for edge in &graph.edges {
            if edge.from == from_node {
                next_nodes.push(edge.to.clone());
            }
        }

        // Check conditional branches
        if let Some(branch) = graph.branches.get(from_node) {
            let target = (branch.condition)(state)?;
            if branch.targets.contains(&target) {
                next_nodes.push(target);
            }
        }

        // Remove duplicates and END nodes
        next_nodes.sort();
        next_nodes.dedup();
        next_nodes.retain(|node| node != END);

        Ok(next_nodes)
    }

    /// Check if should interrupt before nodes
    fn should_interrupt_before(&self, config: &GraphConfig, nodes: &[String]) -> bool {
        if config.interrupt_before.is_empty() {
            return false;
        }
        nodes
            .iter()
            .any(|node| config.interrupt_before.contains(node))
    }

    /// Check if should interrupt after nodes
    fn should_interrupt_after(&self, config: &GraphConfig, nodes: &[String]) -> bool {
        if config.interrupt_after.is_empty() {
            return false;
        }
        nodes
            .iter()
            .any(|node| config.interrupt_after.contains(node))
    }

    /// Send interrupt event
    async fn send_interrupt_event(
        &self,
        execution: &ExecutionState<S>,
        nodes: &[String],
        reason: &str,
    ) -> GraphResult<()> {
        let step = execution.step.load(Ordering::Relaxed);

        let interrupt_state = {
            let state = execution.current_state.read().await;
            state.clone()
        };

        let snapshot = StateSnapshot {
            id: Uuid::new_v4().to_string(),
            state: interrupt_state,
            step,
            next_nodes: nodes.to_vec(),
            timestamp: Utc::now(),
            metadata: HashMap::from([
                ("interrupted".to_string(), serde_json::Value::Bool(true)),
                (
                    "reason".to_string(),
                    serde_json::Value::String(reason.to_string()),
                ),
                (
                    "config".to_string(),
                    serde_json::json!(execution.config.config.clone()),
                ),
            ]),
        };

        if let Some(thread_id) = &execution.context.thread_id {
            self.snapshots
                .entry(thread_id.clone())
                .or_insert_with(Vec::new)
                .push(snapshot);
        }

        let interrupt_event = StreamEvent {
            event_type: StreamEventType::Custom("interrupt".to_string()),
            timestamp: Utc::now(),
            step,
            node: nodes.first().cloned(),
            data: StreamEventData::Custom(serde_json::json!({
                "reason": reason,
                "nodes": nodes
            })),
        };
        self.emit_event(execution, interrupt_event);
        Ok(())
    }

    /// Create a checkpoint
    async fn create_checkpoint(&self, execution: &ExecutionState<S>, step: u32) -> GraphResult<()> {
        let state = {
            let state = execution.current_state.read().await;
            state.clone()
        };

        let next_nodes = {
            let nodes = execution.next_nodes_for_checkpoint.read().await;
            nodes.clone()
        };

        let snapshot = StateSnapshot {
            id: Uuid::new_v4().to_string(),
            state: state.clone(),
            step,
            next_nodes,
            timestamp: Utc::now(),
            metadata: HashMap::from([(
                "config".to_string(),
                serde_json::json!(execution.config.config.clone()),
            )]),
        };

        // Store snapshot
        // TODO: Here we need to call the checkpoint type
        if let Some(thread_id) = &execution.context.thread_id {
            self.snapshots
                .entry(thread_id.clone())
                .or_insert_with(Vec::new)
                .push(snapshot.clone());
        }

        // Send checkpoint event
        let checkpoint_event = StreamEvent {
            event_type: StreamEventType::Checkpoint,
            timestamp: Utc::now(),
            step,
            node: None,
            data: StreamEventData::Checkpoint {
                id: snapshot.id,
                step,
            },
        };
        self.emit_event(execution, checkpoint_event);

        Ok(())
    }

    /// Get current state snapshot
    pub async fn get_state(&self, thread_id: &str) -> GraphResult<Option<StateSnapshot<S>>> {
        if let Some(snapshots) = self.snapshots.get(thread_id) {
            Ok(snapshots.last().cloned())
        } else {
            Ok(None)
        }
    }

    /// Update state
    pub async fn update_state(&self, thread_id: &str, state: S) -> GraphResult<()> {
        // This would typically update the checkpoint/state storage
        // For now, just create a new snapshot
        let snapshot = StateSnapshot {
            id: Uuid::new_v4().to_string(),
            state,
            step: 0, // Would be determined from context
            next_nodes: vec![],
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        };

        self.snapshots
            .entry(thread_id.to_string())
            .or_insert_with(Vec::new)
            .push(snapshot);

        Ok(())
    }

    /// Get state history
    pub async fn get_state_history(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> GraphResult<Vec<StateSnapshot<S>>> {
        if let Some(snapshots) = self.snapshots.get(thread_id) {
            let snapshots = snapshots.clone();
            if let Some(limit) = limit {
                let start = snapshots.len().saturating_sub(limit);
                Ok(snapshots[start..].to_vec())
            } else {
                Ok(snapshots)
            }
        } else {
            Ok(vec![])
        }
    }

    /// Get execution statistics
    pub async fn get_stats(&self) -> ExecutionStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Update execution statistics
    pub async fn update_stats<F>(&self, updater: F)
    where
        F: FnOnce(&mut ExecutionStats),
    {
        let mut stats = self.stats.write().await;
        updater(&mut *stats);
    }
}

impl<S: GraphState> PregelEngine<S> {
    fn emit_event(&self, execution: &ExecutionState<S>, event: StreamEvent<S>) {
        if Self::should_emit_event(&execution.config.stream_mode, &event.event_type) {
            let _ = execution.event_sender.send(event);
        }
    }

    fn should_emit_event(mode: &StreamMode, event_type: &StreamEventType) -> bool {
        let is_terminal = matches!(
            event_type,
            StreamEventType::GraphStart
                | StreamEventType::GraphComplete
                | StreamEventType::GraphError
        );
        if is_terminal {
            return true;
        }

        if matches!(event_type, StreamEventType::Custom(name) if name == "interrupt") {
            return true;
        }

        match mode {
            StreamMode::Values => matches!(
                event_type,
                StreamEventType::StateUpdate | StreamEventType::Checkpoint
            ),
            StreamMode::Updates => matches!(
                event_type,
                StreamEventType::StateUpdate
                    | StreamEventType::NodeError
                    | StreamEventType::Checkpoint
            ),
            StreamMode::Checkpoints => matches!(
                event_type,
                StreamEventType::Checkpoint | StreamEventType::NodeError
            ),
            StreamMode::Tasks => matches!(
                event_type,
                StreamEventType::NodeStart
                    | StreamEventType::NodeComplete
                    | StreamEventType::NodeError
            ),
            StreamMode::Debug | StreamMode::Custom => true,
        }
    }
}
