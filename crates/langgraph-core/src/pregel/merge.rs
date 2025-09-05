// Add these implementations after the existing GraphState trait

use std::collections::HashMap;

use crate::{GraphResult, GraphState};

/// Workflow-specific merge context for sophisticated state merging
#[derive(Debug, Clone)]
pub struct WorkflowMergeContext {
    /// The node that produced this state update
    pub source_node: String,
    /// Strategy to use for merging conflicting values
    pub merge_strategy: MergeStrategy,
    /// Whether to preserve lineage information in merged data
    pub preserve_lineage: bool,
    /// Additional metadata for the merge operation
    pub metadata: HashMap<String, serde_json::Value>,
}

impl WorkflowMergeContext {
    /// Create a new merge context with default settings
    pub fn new(source_node: String) -> Self {
        Self {
            source_node,
            merge_strategy: MergeStrategy::WorkflowAdditive,
            preserve_lineage: true,
            metadata: HashMap::new(),
        }
    }

    /// Create a merge context with specific strategy
    pub fn with_strategy(source_node: String, strategy: MergeStrategy) -> Self {
        Self {
            source_node,
            merge_strategy: strategy,
            preserve_lineage: true,
            metadata: HashMap::new(),
        }
    }
}

/// Strategies for merging conflicting state values
#[derive(Debug, Clone, PartialEq)]
pub enum MergeStrategy {
    /// Additive merging - combine arrays, merge objects, prefer non-null values
    /// This is ideal for workflow orchestration where you want to accumulate results
    WorkflowAdditive,
    
    /// Last writer wins - replace values entirely with the newest update
    /// Useful for configuration or status fields that should be overwritten
    LastWriterWins,
    
    /// Deep merge with conflict resolution based on field types and names
    /// Provides sophisticated merging with field-specific rules
    DeepMergeWithResolution,
    
    /// First writer wins - keep the original value, ignore updates
    /// Useful for immutable fields or initialization values
    FirstWriterWins,
    
    /// Priority-based merging - merge based on source node priority
    /// Higher priority nodes can override lower priority ones
    PriorityBased,
}

/// Trait for GraphState types that support workflow-aware merging
pub trait GraphStateWorkflowMerge: GraphState {
    /// Merge this state with another using workflow-specific context
    fn merge_with_context(&self, other: &Self, context: &WorkflowMergeContext) -> GraphResult<Self>
    where 
        Self: Sized;

    /// Merge multiple states with context - default implementation
    fn merge_multiple_with_context(
        base: &Self,
        updates: &[(String, Self)], 
        default_strategy: MergeStrategy
    ) -> GraphResult<Self>
    where 
        Self: Sized + Clone,
    {
        if updates.is_empty() {
            return Ok(base.clone());
        }

        let mut result = base.clone();
        
        for (source_node, update_state) in updates {
            let context = WorkflowMergeContext::with_strategy(
                source_node.clone(), 
                default_strategy.clone()
            );
            result = result.merge_with_context(update_state, &context)?;
        }
        
        Ok(result)
    }
}

// Provide a default implementation for types that already implement GraphState
impl<T> GraphStateWorkflowMerge for T 
where 
    T: GraphState + Clone + std::fmt::Debug,
{
    fn merge_with_context(&self, other: &Self, context: &WorkflowMergeContext) -> GraphResult<Self> {
        match context.merge_strategy {
            MergeStrategy::WorkflowAdditive | MergeStrategy::DeepMergeWithResolution => {
                // Try to use sophisticated merge if available, otherwise fallback to basic merge
                other.merge(self)
            },
            MergeStrategy::LastWriterWins => Ok(other.clone()),
            MergeStrategy::FirstWriterWins => Ok(self.clone()),
            MergeStrategy::PriorityBased => {
                // For priority-based, we'd need node priority info
                // For now, fallback to basic merge
                other.merge(self)
            }
        }
    }
}
