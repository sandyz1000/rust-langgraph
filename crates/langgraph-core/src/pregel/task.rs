#![allow(unused)]
//! Task representation and execution for Pregel engine

use crate::errors::{GraphResult, LangGraphError};
use crate::types::{ExecutionContext, GraphState, NodeFunction};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

/// Pregel task for node execution
pub struct PregelTask<S: GraphState> {
    /// Task ID
    pub id: String,
    /// Node name
    pub node_name: String,
    /// Input state
    pub input_state: S,
    /// Execution context
    pub context: ExecutionContext,
    /// Node function
    pub function: Arc<dyn NodeFunction<S>>,
    /// Task status
    pub status: TaskStatus,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Start timestamp
    pub started_at: Option<DateTime<Utc>>,
    /// Completion timestamp
    pub completed_at: Option<DateTime<Utc>>,
}

impl<S: GraphState> PregelTask<S> {
    /// Create a new Pregel task
    pub fn new(
        id: String,
        node_name: String,
        input_state: S,
        context: ExecutionContext,
        function: Arc<dyn NodeFunction<S>>,
    ) -> Self {
        Self {
            id,
            node_name,
            input_state,
            context,
            function,
            status: TaskStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
        }
    }

    /// Execute the task
    pub async fn execute(&mut self) -> TaskResult<S> {
        self.status = TaskStatus::Running;
        self.started_at = Some(Utc::now());

        let start_time = Instant::now();
        let result = self.function.call(self.input_state.clone(), self.context.clone()).await;
        let duration = start_time.elapsed();

        self.completed_at = Some(Utc::now());

        match result {
            Ok(output_state) => {
                self.status = TaskStatus::Completed;
                TaskResult {
                    task_id: self.id.clone(),
                    node_name: self.node_name.clone(),
                    status: TaskStatus::Completed,
                    output_state: Some(output_state),
                    error: None,
                    duration_ms: duration.as_millis() as u64,
                    completed_at: self.completed_at.unwrap(),
                }
            }
            Err(err) => {
                self.status = TaskStatus::Failed;
                TaskResult {
                    task_id: self.id.clone(),
                    node_name: self.node_name.clone(),
                    status: TaskStatus::Failed,
                    output_state: None,
                    error: Some(err.to_string()),
                    duration_ms: duration.as_millis() as u64,
                    completed_at: self.completed_at.unwrap(),
                }
            }
        }
    }

    /// Get task duration if completed
    pub fn duration(&self) -> Option<chrono::Duration> {
        if let (Some(start), Some(end)) = (self.started_at, self.completed_at) {
            Some(end - start)
        } else {
            None
        }
    }

    /// Check if task is complete
    pub fn is_complete(&self) -> bool {
        matches!(self.status, TaskStatus::Completed | TaskStatus::Failed)
    }

    /// Check if task failed
    pub fn is_failed(&self) -> bool {
        matches!(self.status, TaskStatus::Failed)
    }

    /// Get task age since creation
    pub fn age(&self) -> chrono::Duration {
        Utc::now() - self.created_at
    }
}

/// Task execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is waiting to be executed
    Pending,
    /// Task is currently running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed with an error
    Failed,
    /// Task was cancelled
    Cancelled,
    /// Task timed out
    TimedOut,
}

impl TaskStatus {
    /// Check if status is terminal (won't change)
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::TimedOut
        )
    }

    /// Check if status indicates success
    pub fn is_successful(self) -> bool {
        matches!(self, TaskStatus::Completed)
    }

    /// Check if status indicates failure
    pub fn is_failed(self) -> bool {
        matches!(
            self,
            TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::TimedOut
        )
    }
}

/// Result of task execution
#[derive(Debug, Clone)]
pub struct TaskResult<S: GraphState> {
    /// Task ID
    pub task_id: String,
    /// Node name
    pub node_name: String,
    /// Execution status
    pub status: TaskStatus,
    /// Output state (if successful)
    pub output_state: Option<S>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Completion timestamp
    pub completed_at: DateTime<Utc>,
}

impl<S: GraphState> TaskResult<S> {
    /// Create a successful task result
    pub fn success(
        task_id: String,
        node_name: String,
        output_state: S,
        duration_ms: u64,
    ) -> Self {
        Self {
            task_id,
            node_name,
            status: TaskStatus::Completed,
            output_state: Some(output_state),
            error: None,
            duration_ms,
            completed_at: Utc::now(),
        }
    }

    /// Create a failed task result
    pub fn failure(
        task_id: String,
        node_name: String,
        error: String,
        duration_ms: u64,
    ) -> Self {
        Self {
            task_id,
            node_name,
            status: TaskStatus::Failed,
            output_state: None,
            error: Some(error),
            duration_ms,
            completed_at: Utc::now(),
        }
    }

    /// Check if result indicates success
    pub fn is_successful(&self) -> bool {
        self.status.is_successful()
    }

    /// Check if result indicates failure
    pub fn is_failed(&self) -> bool {
        self.status.is_failed()
    }

    /// Get the error message if failed
    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Get the output state if successful
    pub fn output(&self) -> Option<&S> {
        self.output_state.as_ref()
    }

    pub fn into_result(self) -> Result<S, LangGraphError> {
        match self.status {
            TaskStatus::Completed => {
                self.output_state.ok_or_else(|| {
                    LangGraphError::runtime("Task completed but no output state")
                })
            }
            TaskStatus::Failed => {
                let error_msg = self.error.unwrap_or_else(|| "Unknown error".to_string());
                Err(LangGraphError::node_execution(self.node_name, error_msg))
            }
            TaskStatus::Cancelled => {
                Err(LangGraphError::runtime("Task was cancelled"))
            }
            TaskStatus::TimedOut => {
                Err(LangGraphError::timeout(format!("Task {} timed out", self.node_name)))
            }
            _ => {
                Err(LangGraphError::runtime(format!(
                    "Task in unexpected state: {:?}",
                    self.status
                )))
            }
        }
    }
}

/// Task priority for scheduling
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaskPriority {
    /// Low priority task
    Low = 0,
    /// Normal priority task
    Normal = 1,
    /// High priority task
    High = 2,
    /// Critical priority task
    Critical = 3,
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Normal
    }
}

impl TaskPriority {
    /// Get priority as numeric value
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Create priority from numeric value
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(TaskPriority::Low),
            1 => Some(TaskPriority::Normal),
            2 => Some(TaskPriority::High),
            3 => Some(TaskPriority::Critical),
            _ => None,
        }
    }
}

/// Task metadata for additional information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetadata {
    /// Task priority
    pub priority: TaskPriority,
    /// Maximum execution time in milliseconds
    pub timeout_ms: Option<u64>,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Current retry attempt
    pub retry_count: u32,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Custom metadata fields
    pub custom: std::collections::HashMap<String, serde_json::Value>,
}

impl Default for TaskMetadata {
    fn default() -> Self {
        Self {
            priority: TaskPriority::Normal,
            timeout_ms: None,
            max_retries: 0,
            retry_count: 0,
            tags: Vec::new(),
            custom: std::collections::HashMap::new(),
        }
    }
}

impl TaskMetadata {
    /// Create new task metadata
    pub fn new() -> Self {
        Self::default()
    }

    /// Set priority
    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Set max retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
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

    /// Check if can retry
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    /// Check if task should timeout
    pub fn should_timeout(&self, duration: std::time::Duration) -> bool {
        if let Some(timeout_ms) = self.timeout_ms {
            duration.as_millis() as u64 > timeout_ms
        } else {
            false
        }
    }
}
