//! Graph definition and execution engine

use crate::channels::{ChannelManager, ChannelSpec, ChannelType, LastValueChannel};
use crate::constants::{END, START};
use crate::errors::{GraphResult, LangGraphError};
use crate::pregel::PregelEngine;
use crate::types::{
    BranchSpec, EdgeSpec, ExecutionContext, GraphConfig, GraphState, NodeFunction, NodeSpec,
    StateSnapshot, StreamEvent,
};
use serde::Serialize;
use std::collections::HashMap;
use tokio_stream::Stream;
use serde_json::Value as JsonValue;

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
    /// Channel specifications
    channel_specs: Vec<ChannelSpec>,
    /// Graph metadata
    metadata: HashMap<String, JsonValue>,
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
            channel_specs: Vec::new(),
            metadata: HashMap::new(),
            compiled: false,
        }
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

    /// Add a channel specification
    pub fn add_channel(&mut self, spec: ChannelSpec) -> GraphResult<&mut Self> {
        if self.compiled {
            return Err(LangGraphError::graph_validation(
                "Cannot modify compiled graph",
            ));
        }

        self.channel_specs.push(spec);
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

        // Create channel manager
        let channel_manager = ChannelManager::new();

        // Setup default channels for state management
        let state_channel = LastValueChannel::<S>::new();
        channel_manager
            .register_channel::<S>("__state__", state_channel)
            .await?;

        // Setup additional channels from specs
        for spec in &self.channel_specs {
            match spec.channel_type {
                ChannelType::LastValue => {
                    let channel = LastValueChannel::<JsonValue>::new();
                    channel_manager
                        .register_channel::<JsonValue>(&spec.name, channel)
                        .await?;
                }
                ChannelType::Ephemeral => {
                    // Handle ephemeral channels
                }
                _ => {
                    // Handle other channel types
                }
            }
        }

        // Create Pregel engine
        let pregel_engine = PregelEngine::new(channel_manager);

        Ok(CompiledGraph {
            nodes: self.nodes,
            edges: self.edges,
            branches: self.branches,
            entry_points: self.entry_points,
            finish_points: self.finish_points,
            metadata: self.metadata,
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
    pub async fn invoke_with_config(&self, input: S, _config: GraphConfig) -> GraphResult<S> {
        // Simple implementation for testing
        // TODO: Replace with proper Pregel execution

        let mut current_state = input;
        let mut step = 0;
        const MAX_STEPS: u32 = 100; // Prevent infinite loops

        let mut current_nodes = self.entry_points.clone();

        while !current_nodes.is_empty() && step < MAX_STEPS {
            step += 1;
            let mut next_nodes = Vec::new();

            // Execute each current node
            for node_name in &current_nodes {
                if node_name == END {
                    continue;
                }

                if let Some(node_spec) = self.nodes.get(node_name) {
                    // Create execution context
                    let mut context = ExecutionContext::new(node_name, "graph");
                    context.step = step;

                    // Execute the node function
                    current_state = node_spec.function.call(current_state, context).await?;

                    // Determine next nodes based on edges and conditional branches
                    let mut found_next = false;

                    // Check conditional branches first
                    if let Some(branch) = self.branches.get(node_name) {
                        let condition_result = (branch.condition)(&current_state)?;
                        if branch.targets.contains(&condition_result) {
                            if condition_result == END {
                                return Ok(current_state);
                            }
                            next_nodes.push(condition_result);
                            found_next = true;
                        }
                    }

                    // Check direct edges if no conditional branch matched
                    if !found_next {
                        for edge in &self.edges {
                            if edge.from == *node_name {
                                if edge.to == END {
                                    return Ok(current_state);
                                }
                                next_nodes.push(edge.to.clone());
                            }
                        }
                    }
                }
            }

            current_nodes = next_nodes;
        }

        if step >= MAX_STEPS {
            return Err(LangGraphError::recursion_limit(MAX_STEPS as usize));
        }

        Ok(current_state)
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
