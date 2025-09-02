//! Constants used throughout LangGraph

/// Special node identifier for the start of the graph
pub const START: &str = "__start__";

/// Special node identifier for the end of the graph
pub const END: &str = "__end__";

/// Default stream mode for graph execution
pub const DEFAULT_STREAM_MODE: &str = "updates";

/// Maximum recursion depth for graph execution
pub const MAX_RECURSION_DEPTH: usize = 1000;

/// Default timeout for node execution (in milliseconds)
pub const DEFAULT_NODE_TIMEOUT_MS: u64 = 30000;

/// Checkpoint version identifier
pub const CHECKPOINT_VERSION: u32 = 1;

/// Namespace separator for nested graphs
pub const NS_SEP: &str = ":";

/// End namespace identifier
pub const NS_END: &str = "__end__";

/// Task identifier for null tasks
pub const NULL_TASK_ID: &str = "00000000-0000-0000-0000-000000000000";

/// Channel for interrupts
pub const INTERRUPT: &str = "__interrupt__";

/// Channel for errors
pub const ERROR: &str = "__error__";

/// Channel for inputs
pub const INPUT: &str = "__input__";

/// Channel for tasks
pub const TASKS: &str = "__tasks__";

/// Channel for push operations
pub const PUSH: &str = "__push__";
