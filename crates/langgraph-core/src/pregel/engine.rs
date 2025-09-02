//! Core Pregel execution engine implementation

use crate::channels::ChannelManager;
use crate::constants::{START, END, MAX_RECURSION_DEPTH};
use crate::errors::{GraphResult, LangGraphError};
use crate::graph::CompiledGraph;
use crate::pregel::{PregelTask, TaskScheduler, TaskStatus, TaskResult};
use crate::types::{
    ExecutionContext, GraphConfig, GraphState, StateSnapshot, StreamEvent, StreamEventData,
    StreamEventType, StreamMode, ExecutionStats, MemoryStats,
};
use chrono::Utc;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use tokio::sync::{RwLock, mpsc, broadcast};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use uuid::Uuid;

/// Pregel execution engine for graph computation
pub struct PregelEngine<S: GraphState> {
    /// Channel manager for inter-node communication
    channel_manager: ChannelManager,
    /// Task scheduler
    scheduler: TaskScheduler<S>,
    /// Active executions
    executions: Arc<DashMap<String, ExecutionState<S>>>,
    /// State snapshots by thread ID
    snapshots: Arc<DashMap<String, Vec<StateSnapshot<S>>>>,
    /// Execution statistics
    stats: Arc<RwLock<ExecutionStats>>,
}

/// State of an active execution
struct ExecutionState<S: GraphState> {
    /// Execution ID
    execution_id: String,
    /// Current state
    current_state: Arc<RwLock<S>>,
    /// Current step
    step: AtomicU32,
    /// Active tasks
    active_tasks: Arc<DashMap<String, PregelTask<S>>>,
    /// Execution context
    context: ExecutionContext,
    /// Configuration
    config: GraphConfig,
    /// Event sender
    event_sender: broadcast::Sender<StreamEvent<S>>,
}

impl<S: GraphState> PregelEngine<S> {
    /// Create a new Pregel engine
    pub fn new(channel_manager: ChannelManager) -> Self {
        Self {
            channel_manager,
            scheduler: TaskScheduler::new(),
            executions: Arc::new(DashMap::new()),
            snapshots: Arc::new(DashMap::new()),
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
        let thread_id = config.thread_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
        
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
            execution_id: execution_id.clone(),
            current_state: Arc::new(RwLock::new(input.clone())),
            step: AtomicU32::new(0),
            active_tasks: Arc::new(DashMap::new()),
            context,
            config: config.clone(),
            event_sender: event_sender.clone(),
        };

        // Store execution state
        self.executions.insert(execution_id.clone(), execution_state);

        // Create initial snapshot
        let initial_snapshot = StateSnapshot {
            id: Uuid::new_v4().to_string(),
            state: input.clone(),
            step: 0,
            next_nodes: graph.entry_points.clone(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        };

        // Store snapshot
        self.snapshots
            .entry(thread_id.clone())
            .or_insert_with(Vec::new)
            .push(initial_snapshot);

        // Send graph start event
        let start_event = StreamEvent {
            event_type: StreamEventType::GraphStart,
            timestamp: Utc::now(),
            step: 0,
            node: None,
            data: StreamEventData::State(input.clone()),
        };
        let _ = event_sender.send(start_event);

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

    /// Execute the graph logic
    async fn execute_graph(
        &self,
        graph: &CompiledGraph<S>,
        execution_id: &str,
    ) -> GraphResult<()> {
        let execution = self.executions.get(execution_id)
            .ok_or_else(|| LangGraphError::runtime("Execution not found"))?;

        let mut current_nodes = graph.entry_points.clone();
        let mut step = 0u32;

        while !current_nodes.is_empty() && step < MAX_RECURSION_DEPTH as u32 {
            step += 1;
            execution.step.store(step, Ordering::Relaxed);

            // Check for interrupts
            if self.should_interrupt_before(&execution.config, &current_nodes) {
                self.send_interrupt_event(&execution, &current_nodes, "before").await?;
                break;
            }

            // Execute current nodes
            let next_nodes = self.execute_step(graph, execution_id, &current_nodes).await?;

            // Check for interrupts after
            if self.should_interrupt_after(&execution.config, &current_nodes) {
                self.send_interrupt_event(&execution, &current_nodes, "after").await?;
                break;
            }

            // Update current nodes for next iteration
            current_nodes = next_nodes;

            // Check if we've reached finish points
            if current_nodes.iter().any(|node| graph.finish_points.contains(node) || node == END) {
                break;
            }

            // Create checkpoint if configured
            if execution.config.checkpointing {
                self.create_checkpoint(&execution, step).await?;
            }
        }

        if step >= MAX_RECURSION_DEPTH as u32 {
            return Err(LangGraphError::recursion_limit(MAX_RECURSION_DEPTH));
        }

        // Send completion event
        let final_state = {
            let state = execution.current_state.read().await;
            state.clone()
        };

        let complete_event = StreamEvent {
            event_type: StreamEventType::GraphComplete,
            timestamp: Utc::now(),
            step,
            node: None,
            data: StreamEventData::State(final_state),
        };
        let _ = execution.event_sender.send(complete_event);

        // Clean up execution
        self.executions.remove(execution_id);

        Ok(())
    }

    /// Execute a single step (all current nodes)
    async fn execute_step(
        &self,
        graph: &CompiledGraph<S>,
        execution_id: &str,
        current_nodes: &[String],
    ) -> GraphResult<Vec<String>> {
        let execution = self.executions.get(execution_id)
            .ok_or_else(|| LangGraphError::runtime("Execution not found"))?;

        let mut next_nodes = Vec::new();
        let mut tasks = Vec::new();

        // Create tasks for each node
        for node_name in current_nodes {
            if let Some(node_spec) = graph.nodes.get(node_name) {
                let current_state = {
                    let state = execution.current_state.read().await;
                    state.clone()
                };

                let mut task_context = execution.context.clone();
                task_context.node_name = node_name.clone();
                task_context.step = execution.step.load(Ordering::Relaxed);

                let task = PregelTask::new(
                    Uuid::new_v4().to_string(),
                    node_name.clone(),
                    current_state,
                    task_context,
                    node_spec.function.clone(),
                );

                tasks.push(task);
            }
        }

        // Execute tasks
        let task_results = self.scheduler.execute_tasks(tasks).await?;

        // Process results
        for result in task_results {
            match result.status {
                TaskStatus::Completed => {
                    if let Some(new_state) = result.output_state {
                        // Update current state
                        {
                            let mut state = execution.current_state.write().await;
                            *state = new_state.clone();
                        }

                        // Send state update event
                        let update_event = StreamEvent {
                            event_type: StreamEventType::StateUpdate,
                            timestamp: Utc::now(),
                            step: execution.step.load(Ordering::Relaxed),
                            node: Some(result.node_name.clone()),
                            data: StreamEventData::State(new_state.clone()),
                        };
                        let _ = execution.event_sender.send(update_event);

                        // Determine next nodes
                        let next = self.get_next_nodes(graph, &result.node_name, &new_state).await?;
                        next_nodes.extend(next);
                    }

                    // Send node complete event
                    let complete_event = StreamEvent {
                        event_type: StreamEventType::NodeComplete,
                        timestamp: Utc::now(),
                        step: execution.step.load(Ordering::Relaxed),
                        node: Some(result.node_name),
                        data: StreamEventData::Custom(serde_json::json!({
                            "duration_ms": result.duration_ms
                        })),
                    };
                    let _ = execution.event_sender.send(complete_event);
                }
                TaskStatus::Failed => {
                    // Send error event
                    let error_event = StreamEvent {
                        event_type: StreamEventType::NodeError,
                        timestamp: Utc::now(),
                        step: execution.step.load(Ordering::Relaxed),
                        node: Some(result.node_name.clone()),
                        data: StreamEventData::Error {
                            message: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                            code: 1001,
                        },
                    };
                    let _ = execution.event_sender.send(error_event);

                    return Err(LangGraphError::node_execution(
                        result.node_name,
                        "Node execution failed",
                    ));
                }
                _ => {}
            }
        }

        Ok(next_nodes)
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
        nodes.iter().any(|node| config.interrupt_before.contains(node))
    }

    /// Check if should interrupt after nodes
    fn should_interrupt_after(&self, config: &GraphConfig, nodes: &[String]) -> bool {
        if config.interrupt_after.is_empty() {
            return false;
        }
        nodes.iter().any(|node| config.interrupt_after.contains(node))
    }

    /// Send interrupt event
    async fn send_interrupt_event(
        &self,
        execution: &ExecutionState<S>,
        nodes: &[String],
        when: &str,
    ) -> GraphResult<()> {
        let interrupt_event = StreamEvent {
            event_type: StreamEventType::Custom("interrupt".to_string()),
            timestamp: Utc::now(),
            step: execution.step.load(Ordering::Relaxed),
            node: nodes.first().cloned(),
            data: StreamEventData::Custom(serde_json::json!({
                "when": when,
                "nodes": nodes
            })),
        };
        let _ = execution.event_sender.send(interrupt_event);
        Ok(())
    }

    /// Create a checkpoint
    async fn create_checkpoint(
        &self,
        execution: &ExecutionState<S>,
        step: u32,
    ) -> GraphResult<()> {
        let state = {
            let state = execution.current_state.read().await;
            state.clone()
        };

        let snapshot = StateSnapshot {
            id: Uuid::new_v4().to_string(),
            state: state.clone(),
            step,
            next_nodes: vec![], // Would be populated with actual next nodes
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        };

        // Store snapshot
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
        let _ = execution.event_sender.send(checkpoint_event);

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
