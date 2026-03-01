//! Graph definition and execution engine

use crate::channels::{ChannelSpec, ChannelType};
use crate::constants::{END, START};
use crate::errors::{GraphResult, LangGraphError};
use crate::pregel::PregelEngine;
use crate::types::{
    BranchSpec, EdgeSpec, GraphConfig, GraphState, InterruptInfo, InvokeOutcome, NodeFunction,
    NodeSpec, StateSnapshot, StreamEvent, StreamEventData, StreamEventType,
};
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_stream::{Stream, StreamExt};

/// State graph builder for creating computational graphs
pub struct StateGraph<S: GraphState> {
    /// Graph nodes
    nodes: HashMap<String, NodeSpec<S>>,
    /// Direct edges between nodes
    edges: Vec<EdgeSpec>,
    /// Conditional branches from nodes
    branches: HashMap<String, BranchSpec<S>>,
    /// Entry points to the graph
    entry_points: Vec<String>,
    /// Finish points of the graph
    finish_points: Vec<String>,
    /// Graph metadata
    metadata: HashMap<String, JsonValue>,
    /// Optional channel behavior per state key
    channels: HashMap<String, ChannelSpec>,
    /// Whether the graph has been compiled
    compiled: bool,
}

impl<S: GraphState> StateGraph<S> {
    /// Create a new state graph
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            branches: HashMap::new(),
            entry_points: Vec::new(),
            finish_points: Vec::new(),
            metadata: HashMap::new(),
            channels: HashMap::new(),
            compiled: false,
        }
    }

    /// Configure channel behavior for a state key.
    pub fn set_channel(&mut self, spec: ChannelSpec) -> GraphResult<&mut Self> {
        if self.compiled {
            return Err(LangGraphError::graph_validation(
                "Cannot modify compiled graph",
            ));
        }

        self.channels.insert(spec.name.clone(), spec);
        Ok(self)
    }

    /// Configure channel type for a state key.
    pub fn set_channel_type(
        &mut self,
        key: impl Into<String>,
        channel_type: ChannelType,
    ) -> GraphResult<&mut Self> {
        self.set_channel(ChannelSpec::new(key, channel_type))
    }

    /// Add a node to the graph
    pub fn add_node<F>(&mut self, name: impl Into<String>, function: F) -> GraphResult<&mut Self>
    where
        F: NodeFunction<S> + 'static,
    {
        let name = name.into();
        if self.compiled {
            return Err(LangGraphError::graph_validation(
                "Cannot modify compiled graph",
            ));
        }

        if self.nodes.contains_key(&name) {
            return Err(LangGraphError::graph_validation(format!(
                "Node '{}' already exists",
                name
            )));
        }

        let node_spec = NodeSpec::new(name.clone(), function);
        self.nodes.insert(name, node_spec);
        Ok(self)
    }

    /// Add a direct edge between two nodes
    pub fn add_edge(
        &mut self,
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> GraphResult<&mut Self> {
        if self.compiled {
            return Err(LangGraphError::graph_validation(
                "Cannot modify compiled graph",
            ));
        }

        let from = from.into();
        let to = to.into();

        // Validate nodes exist (except for START and END)
        if from != START && !self.nodes.contains_key(&from) {
            return Err(LangGraphError::node_not_found(&from));
        }
        if to != END && !self.nodes.contains_key(&to) {
            return Err(LangGraphError::node_not_found(&to));
        }

        // If the edge starts from START, automatically add the target as an entry point
        if from == START && to != END && !self.entry_points.contains(&to) {
            self.entry_points.push(to.clone());
        }

        self.edges.push(EdgeSpec {
            from,
            to,
            condition: None,
        });
        Ok(self)
    }

    /// Add a conditional edge with branching logic
    pub fn add_conditional_edge<F>(
        &mut self,
        from: impl Into<String>,
        condition: F,
        targets: Vec<String>,
    ) -> GraphResult<&mut Self>
    where
        F: Fn(&S) -> GraphResult<String> + Send + Sync + 'static,
    {
        if self.compiled {
            return Err(LangGraphError::graph_validation(
                "Cannot modify compiled graph",
            ));
        }

        let from = from.into();

        // Validate source node exists
        if from != START && !self.nodes.contains_key(&from) {
            return Err(LangGraphError::node_not_found(&from));
        }

        // Validate target nodes exist
        for target in &targets {
            if target != END && !self.nodes.contains_key(target) {
                return Err(LangGraphError::node_not_found(target));
            }
        }

        // If the conditional edge starts from START, add all non-END targets as entry points
        if from == START {
            for target in &targets {
                if target != END && !self.entry_points.contains(target) {
                    self.entry_points.push(target.clone());
                }
            }
        }

        let branch_spec = BranchSpec::new(condition, targets);
        self.branches.insert(from, branch_spec);
        Ok(self)
    }

    /// Add a sequence of nodes with automatic edges
    pub fn add_sequence<F>(&mut self, sequence: Vec<(String, F)>) -> GraphResult<&mut Self>
    where
        F: NodeFunction<S> + 'static,
    {
        if sequence.is_empty() {
            return Ok(self);
        }

        let mut prev_node = None;
        for (name, function) in sequence {
            self.add_node(name.clone(), function)?;

            if let Some(prev) = prev_node {
                self.add_edge(prev, name.clone())?;
            }
            prev_node = Some(name);
        }

        Ok(self)
    }

    /// Add a compiled subgraph as a node with explicit state transforms.
    pub fn add_subgraph_with_transform<SubS, ToSub, FromSub>(
        &mut self,
        name: impl Into<String>,
        subgraph: CompiledGraph<SubS>,
        to_sub_state: ToSub,
        from_sub_state: FromSub,
    ) -> GraphResult<&mut Self>
    where
        SubS: GraphState,
        ToSub: Fn(&S) -> GraphResult<SubS> + Send + Sync + Clone + 'static,
        FromSub:
            Fn(&S, &SubS) -> GraphResult<crate::types::StateUpdate> + Send + Sync + Clone + 'static,
    {
        let name = name.into();
        let subgraph = Arc::new(subgraph);

        let node_fn = move |state: S, _context: crate::types::ExecutionContext| {
            let subgraph = Arc::clone(&subgraph);
            let to_sub_state = to_sub_state.clone();
            let from_sub_state = from_sub_state.clone();

            async move {
                let sub_input = to_sub_state(&state)?;
                let sub_output = subgraph.invoke(sub_input).await?;
                from_sub_state(&state, &sub_output)
            }
        };

        self.add_node(name, node_fn)
    }

    /// Set the entry point of the graph
    pub fn set_entry_point(&mut self, node: impl Into<String>) -> GraphResult<&mut Self> {
        if self.compiled {
            return Err(LangGraphError::graph_validation(
                "Cannot modify compiled graph",
            ));
        }

        let node = node.into();
        if !self.nodes.contains_key(&node) {
            return Err(LangGraphError::node_not_found(&node));
        }

        self.entry_points.clear();
        self.entry_points.push(node);
        Ok(self)
    }

    /// Add an entry point to the graph
    pub fn add_entry_point(&mut self, node: impl Into<String>) -> GraphResult<&mut Self> {
        if self.compiled {
            return Err(LangGraphError::graph_validation(
                "Cannot modify compiled graph",
            ));
        }

        let node = node.into();
        if !self.nodes.contains_key(&node) {
            return Err(LangGraphError::node_not_found(&node));
        }

        if !self.entry_points.contains(&node) {
            self.entry_points.push(node);
        }
        Ok(self)
    }

    /// Set the finish point of the graph
    pub fn set_finish_point(&mut self, node: impl Into<String>) -> GraphResult<&mut Self> {
        if self.compiled {
            return Err(LangGraphError::graph_validation(
                "Cannot modify compiled graph",
            ));
        }

        let node = node.into();
        if !self.nodes.contains_key(&node) {
            return Err(LangGraphError::node_not_found(&node));
        }

        self.finish_points.clear();
        self.finish_points.push(node);
        Ok(self)
    }

    /// Add a finish point to the graph
    pub fn add_finish_point(&mut self, node: impl Into<String>) -> GraphResult<&mut Self> {
        if self.compiled {
            return Err(LangGraphError::graph_validation(
                "Cannot modify compiled graph",
            ));
        }

        let node = node.into();
        if !self.nodes.contains_key(&node) {
            return Err(LangGraphError::node_not_found(&node));
        }

        if !self.finish_points.contains(&node) {
            self.finish_points.push(node);
        }
        Ok(self)
    }

    /// Set graph metadata
    pub fn set_metadata<T>(&mut self, key: impl Into<String>, value: T) -> GraphResult<&mut Self>
    where
        T: Serialize,
    {
        let json_value = serde_json::to_value(value)?;
        self.metadata.insert(key.into(), json_value);
        Ok(self)
    }

    /// Validate the graph structure
    pub fn validate(&self) -> GraphResult<()> {
        // Check that we have entry points
        if self.entry_points.is_empty() {
            return Err(LangGraphError::graph_validation(
                "Graph must have at least one entry point",
            ));
        }

        // Check that we have nodes
        if self.nodes.is_empty() {
            return Err(LangGraphError::graph_validation(
                "Graph must have at least one node",
            ));
        }

        // Note: Cycles are allowed in LangGraph for agent workflows
        // They represent valid patterns like reasoning -> tool_execution -> reasoning

        // Check that all referenced nodes exist
        for edge in &self.edges {
            if edge.from != START && !self.nodes.contains_key(&edge.from) {
                return Err(LangGraphError::node_not_found(&edge.from));
            }
            if edge.to != END && !self.nodes.contains_key(&edge.to) {
                return Err(LangGraphError::node_not_found(&edge.to));
            }
        }

        // Check conditional edges
        for (from, branch) in &self.branches {
            if from != START && !self.nodes.contains_key(from) {
                return Err(LangGraphError::node_not_found(from));
            }
            for target in &branch.targets {
                if target != END && !self.nodes.contains_key(target) {
                    return Err(LangGraphError::node_not_found(target));
                }
            }
        }

        Ok(())
    }

    /// Compile the graph into an executable form
    pub async fn compile(mut self) -> GraphResult<CompiledGraph<S>> {
        self.validate()?;
        self.compiled = true;

        // Create Pregel engine
        let pregel_engine = PregelEngine::new();

        Ok(CompiledGraph {
            nodes: self.nodes,
            edges: self.edges,
            branches: self.branches,
            entry_points: self.entry_points,
            finish_points: self.finish_points,
            metadata: self.metadata,
            channels: self.channels,
            pregel_engine,
        })
    }
}

impl<S: GraphState> Default for StateGraph<S> {
    fn default() -> Self {
        Self::new()
    }
}

/// Compiled and executable graph
pub struct CompiledGraph<S: GraphState> {
    /// Graph nodes
    pub(crate) nodes: HashMap<String, NodeSpec<S>>,
    /// Direct edges
    pub(crate) edges: Vec<EdgeSpec>,
    /// Conditional branches
    pub(crate) branches: HashMap<String, BranchSpec<S>>,
    /// Entry points
    pub(crate) entry_points: Vec<String>,
    /// Finish points
    pub(crate) finish_points: Vec<String>,
    /// Graph metadata
    pub(crate) metadata: HashMap<String, JsonValue>,
    /// Channel behavior for state keys
    pub(crate) channels: HashMap<String, ChannelSpec>,
    /// Pregel execution engine
    pub(crate) pregel_engine: PregelEngine<S>,
}

impl<S: GraphState> CompiledGraph<S> {
    /// Execute the graph with the given input state
    pub async fn invoke(&self, input: S) -> GraphResult<S> {
        let config = GraphConfig::default();
        self.invoke_with_config(input, config).await
    }

    /// Execute the graph with configuration
    pub async fn invoke_with_config(&self, input: S, config: GraphConfig) -> GraphResult<S> {
        match self.invoke_outcome_with_config(input, config).await? {
            InvokeOutcome::Completed(state) => Ok(state),
            InvokeOutcome::Interrupted(interrupt) => Err(LangGraphError::interrupted(
                interrupt.node.unwrap_or_else(|| "unknown".to_string()),
                interrupt.reason,
            )),
        }
    }

    /// Execute the graph and return whether it completed or interrupted.
    pub async fn invoke_outcome_with_config(
        &self,
        input: S,
        config: GraphConfig,
    ) -> GraphResult<InvokeOutcome<S>> {
        let thread_id = config.thread_id.clone();
        let mut stream = self.pregel_engine.execute(self, input, config).await?;
        let mut last_state: Option<S> = None;
        let mut interrupt_info: Option<InterruptInfo<S>> = None;

        while let Some(event) = stream.next().await {
            if let StreamEventData::State(state) = &event.data {
                last_state = Some(state.clone());
            }

            match &event.event_type {
                StreamEventType::GraphComplete => {
                    let final_state = last_state.ok_or_else(|| {
                        LangGraphError::runtime("Graph completed without final state")
                    })?;
                    return Ok(InvokeOutcome::Completed(final_state));
                }
                StreamEventType::GraphError => {
                    return Err(LangGraphError::runtime("Graph execution failed"));
                }
                StreamEventType::Custom(name) if name == "interrupt" => {
                    let (reason, next_nodes) = match &event.data {
                        StreamEventData::Custom(value) => {
                            let reason = value
                                .get("reason")
                                .and_then(|v| v.as_str())
                                .unwrap_or("interrupted")
                                .to_string();
                            let next_nodes = value
                                .get("nodes")
                                .and_then(|v| v.as_array())
                                .map(|nodes| {
                                    nodes
                                        .iter()
                                        .filter_map(|node| node.as_str().map(str::to_string))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            (reason, next_nodes)
                        }
                        _ => ("interrupted".to_string(), Vec::new()),
                    };

                    if let Some(state) = last_state.clone() {
                        interrupt_info = Some(InterruptInfo {
                            state,
                            step: event.step,
                            node: event.node.clone(),
                            reason,
                            next_nodes,
                            thread_id: thread_id.clone(),
                        });
                    }
                }
                _ => {}
            }
        }

        if let Some(interrupt) = interrupt_info {
            return Ok(InvokeOutcome::Interrupted(interrupt));
        }

        Err(LangGraphError::runtime(
            "Graph execution ended unexpectedly",
        ))
    }

    /// Execute the graph and return whether it completed or interrupted.
    pub async fn invoke_outcome(&self, input: S) -> GraphResult<InvokeOutcome<S>> {
        let config = GraphConfig::default();
        self.invoke_outcome_with_config(input, config).await
    }

    /// Resume a previously interrupted thread using its latest snapshot.
    pub async fn resume_with_config(
        &self,
        thread_id: &str,
        mut config: GraphConfig,
    ) -> GraphResult<InvokeOutcome<S>> {
        let snapshot = self
            .get_state(thread_id)
            .await?
            .ok_or_else(|| LangGraphError::runtime("No state snapshot found for thread"))?;

        config.thread_id = Some(thread_id.to_string());
        if !snapshot.next_nodes.is_empty() {
            config.resume_next_nodes = Some(snapshot.next_nodes.clone());
        }

        self.invoke_outcome_with_config(snapshot.state, config)
            .await
    }

    /// Resume a previously interrupted thread using default configuration.
    pub async fn resume(&self, thread_id: &str) -> GraphResult<InvokeOutcome<S>> {
        let config = GraphConfig::new().with_thread_id(thread_id.to_string());
        self.resume_with_config(thread_id, config).await
    }
    /// Stream graph execution events
    pub async fn stream(&self, input: S) -> GraphResult<impl Stream<Item = StreamEvent<S>> + '_> {
        let config = GraphConfig::default();
        self.stream_with_config(input, config).await
    }

    /// Stream graph execution with configuration
    pub async fn stream_with_config(
        &self,
        input: S,
        config: GraphConfig,
    ) -> GraphResult<impl Stream<Item = StreamEvent<S>> + '_> {
        self.pregel_engine.execute(self, input, config).await
    }

    /// Get the current state snapshot
    pub async fn get_state(&self, thread_id: &str) -> GraphResult<Option<StateSnapshot<S>>> {
        self.pregel_engine.get_state(thread_id).await
    }

    /// Update the state
    pub async fn update_state(&self, thread_id: &str, state: S) -> GraphResult<()> {
        self.pregel_engine.update_state(thread_id, state).await
    }

    /// Get state history
    pub async fn get_state_history(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> GraphResult<Vec<StateSnapshot<S>>> {
        self.pregel_engine.get_state_history(thread_id, limit).await
    }

    /// Get graph metadata
    pub fn get_metadata(&self, key: &str) -> Option<&JsonValue> {
        self.metadata.get(key)
    }

    /// Get channel specification for a state key.
    pub fn get_channel(&self, key: &str) -> Option<&ChannelSpec> {
        self.channels.get(key)
    }

    /// List configured state channels.
    pub fn list_channels(&self) -> Vec<&str> {
        self.channels.keys().map(|s| s.as_str()).collect()
    }

    /// List all nodes
    pub fn list_nodes(&self) -> Vec<&str> {
        self.nodes.keys().map(|s| s.as_str()).collect()
    }

    /// List all edges
    pub fn list_edges(&self) -> Vec<(&str, &str)> {
        self.edges
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect()
    }

    /// Get entry points
    pub fn entry_points(&self) -> &[String] {
        &self.entry_points
    }

    /// Get finish points
    pub fn finish_points(&self) -> &[String] {
        &self.finish_points
    }

    /// Check if a node exists
    pub fn has_node(&self, name: &str) -> bool {
        self.nodes.contains_key(name)
    }

    /// Get the next nodes from a given node
    pub fn get_next_nodes(&self, from_node: &str) -> Vec<String> {
        let mut next_nodes = Vec::new();

        // Check direct edges
        for edge in &self.edges {
            if edge.from == from_node {
                next_nodes.push(edge.to.clone());
            }
        }

        // Check conditional branches
        if let Some(branch) = self.branches.get(from_node) {
            next_nodes.extend(branch.targets.clone());
        }

        next_nodes
    }

    /// Get the previous nodes to a given node
    pub fn get_previous_nodes(&self, to_node: &str) -> Vec<String> {
        let mut prev_nodes = Vec::new();

        // Check direct edges
        for edge in &self.edges {
            if edge.to == to_node {
                prev_nodes.push(edge.from.clone());
            }
        }

        // Check conditional branches
        for (from_node, branch) in &self.branches {
            if branch.targets.contains(&to_node.to_string()) {
                prev_nodes.push(from_node.clone());
            }
        }

        prev_nodes
    }
}
