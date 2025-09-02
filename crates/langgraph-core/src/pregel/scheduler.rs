//! Task scheduler for concurrent execution

use crate::errors::{GraphResult, LangGraphError};
use crate::pregel::{PregelTask, TaskResult, TaskStatus};
use crate::pregel::task::TaskPriority;
use crate::types::GraphState;
use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore, mpsc};
use tokio::time::timeout;

/// Task scheduler for managing concurrent task execution
#[derive(Debug)]
pub struct TaskScheduler<S: GraphState> {
    /// Maximum concurrent tasks
    max_concurrent_tasks: usize,
    /// Task execution semaphore
    semaphore: Arc<Semaphore>,
    /// Active tasks
    active_tasks: Arc<RwLock<HashMap<String, TaskHandle<S>>>>,
    /// Task queue
    task_queue: Arc<RwLock<TaskQueue>>,
    /// Scheduler configuration
    config: SchedulerConfig,
}

/// Task queue with priority support
#[derive(Debug)]
struct TaskQueue {
    /// High priority tasks
    high_priority: VecDeque<QueuedTask>,
    /// Normal priority tasks
    normal_priority: VecDeque<QueuedTask>,
    /// Low priority tasks
    low_priority: VecDeque<QueuedTask>,
}

/// Queued task wrapper
#[derive(Debug)]
struct QueuedTask {
    /// Task ID
    id: String,
    /// Task priority
    priority: TaskPriority,
    /// Queued timestamp
    queued_at: Instant,
}

/// Scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum concurrent tasks
    pub max_concurrent_tasks: usize,
    /// Task timeout in milliseconds
    pub default_timeout_ms: u64,
    /// Enable task retries
    pub enable_retries: bool,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
    /// Enable fair scheduling
    pub fair_scheduling: bool,
    /// Queue size limits per priority
    pub queue_limits: QueueLimits,
}

/// Queue size limits
#[derive(Debug, Clone)]
pub struct QueueLimits {
    /// High priority queue limit
    pub high_priority: usize,
    /// Normal priority queue limit
    pub normal_priority: usize,
    /// Low priority queue limit
    pub low_priority: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 4,
            default_timeout_ms: 30000, // 30 seconds
            enable_retries: true,
            max_retries: 3,
            retry_delay_ms: 1000,
            fair_scheduling: true,
            queue_limits: QueueLimits::default(),
        }
    }
}

impl Default for QueueLimits {
    fn default() -> Self {
        Self {
            high_priority: 1000,
            normal_priority: 5000,
            low_priority: 10000,
        }
    }
}

impl TaskQueue {
    /// Create a new task queue
    fn new() -> Self {
        Self {
            high_priority: VecDeque::new(),
            normal_priority: VecDeque::new(),
            low_priority: VecDeque::new(),
        }
    }

    /// Add a task to the appropriate queue
    fn enqueue(&mut self, task: QueuedTask, limits: &QueueLimits) -> GraphResult<()> {
        match task.priority {
            TaskPriority::Critical | TaskPriority::High => {
                if self.high_priority.len() >= limits.high_priority {
                    return Err(LangGraphError::runtime("High priority queue full"));
                }
                self.high_priority.push_back(task);
            }
            TaskPriority::Normal => {
                if self.normal_priority.len() >= limits.normal_priority {
                    return Err(LangGraphError::runtime("Normal priority queue full"));
                }
                self.normal_priority.push_back(task);
            }
            TaskPriority::Low => {
                if self.low_priority.len() >= limits.low_priority {
                    return Err(LangGraphError::runtime("Low priority queue full"));
                }
                self.low_priority.push_back(task);
            }
        }
        Ok(())
    }

    /// Dequeue the next task based on priority
    fn dequeue(&mut self) -> Option<QueuedTask> {
        // Try high priority first
        if let Some(task) = self.high_priority.pop_front() {
            return Some(task);
        }

        // Then normal priority
        if let Some(task) = self.normal_priority.pop_front() {
            return Some(task);
        }

        // Finally low priority
        self.low_priority.pop_front()
    }

    /// Get total queue size
    fn len(&self) -> usize {
        self.high_priority.len() + self.normal_priority.len() + self.low_priority.len()
    }

    /// Check if queue is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get queue statistics
    fn stats(&self) -> QueueStats {
        QueueStats {
            high_priority_count: self.high_priority.len(),
            normal_priority_count: self.normal_priority.len(),
            low_priority_count: self.low_priority.len(),
            total_count: self.len(),
        }
    }
}

/// Queue statistics
#[derive(Debug, Clone)]
pub struct QueueStats {
    /// High priority task count
    pub high_priority_count: usize,
    /// Normal priority task count
    pub normal_priority_count: usize,
    /// Low priority task count
    pub low_priority_count: usize,
    /// Total task count
    pub total_count: usize,
}

impl<S: GraphState> TaskScheduler<S> {
    /// Create a new task scheduler
    pub fn new() -> Self {
        let config = SchedulerConfig::default();
        Self::with_config(config)
    }

    /// Create a task scheduler with custom configuration
    pub fn with_config(config: SchedulerConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_tasks));
        Self {
            max_concurrent_tasks: config.max_concurrent_tasks,
            semaphore,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            task_queue: Arc::new(RwLock::new(TaskQueue::new())),
            config,
        }
    }

    /// Execute a batch of tasks
    pub async fn execute_tasks(
        &self,
        tasks: Vec<PregelTask<S>>,
    ) -> GraphResult<Vec<TaskResult<S>>> {
        let mut results = Vec::with_capacity(tasks.len());
        let mut join_handles = Vec::new();

        // Start all tasks
        for task in tasks {
            let task_result = self.execute_single_task(task).await;
            join_handles.push(task_result);
        }

        // Collect results
        for join_result in join_handles {
            results.push(join_result?);
        }

        Ok(results)
    }

    /// Execute a single task directly
    async fn execute_single_task(&self, mut task: PregelTask<S>) -> GraphResult<TaskResult<S>> {
        // Acquire semaphore permit
        let _permit = self.semaphore.acquire().await.unwrap();

        // Execute with timeout
        let execution_timeout = Duration::from_millis(self.config.default_timeout_ms);
        let result = match timeout(execution_timeout, task.execute()).await {
            Ok(result) => result,
            Err(_) => TaskResult {
                task_id: task.id.clone(),
                node_name: task.node_name.clone(),
                status: TaskStatus::TimedOut,
                output_state: None,
                error: Some("Task execution timed out".to_string()),
                duration_ms: execution_timeout.as_millis() as u64,
                completed_at: chrono::Utc::now(),
            },
        };

        Ok(result)
    }

    /// Schedule a single task for execution
    pub async fn schedule_task(
        &self,
        mut task: PregelTask<S>,
    ) -> GraphResult<TaskHandle<S>> {
        let task_id = task.id.clone();
        let priority = TaskPriority::Normal; // Default priority

        // Create task handle
        let (cancel_sender, _cancel_receiver) = mpsc::channel::<()>(1);

        let handle = TaskHandle {
            id: task_id.clone(),
            priority,
            started_at: Instant::now(),
            cancel_sender: cancel_sender.clone(),
            _phantom: PhantomData,
        };

        // Store active task (clone for the map)
        {
            let mut active_tasks = self.active_tasks.write().await;
            let stored_handle = TaskHandle {
                id: task_id.clone(),
                priority,
                started_at: handle.started_at,
                cancel_sender: cancel_sender.clone(),
                _phantom: PhantomData,
            };
            active_tasks.insert(task_id.clone(), stored_handle);
        }

        // Execute task
        let semaphore = self.semaphore.clone();
        let config = self.config.clone();
        let active_tasks = self.active_tasks.clone();

        tokio::spawn(async move {
            // Acquire semaphore permit
            let _permit = semaphore.acquire().await.unwrap();

            // Execute with timeout
            let execution_timeout = Duration::from_millis(config.default_timeout_ms);
            let _result = match timeout(execution_timeout, task.execute()).await {
                Ok(result) => result,
                Err(_) => TaskResult {
                    task_id: task.id.clone(),
                    node_name: task.node_name.clone(),
                    status: TaskStatus::TimedOut,
                    output_state: None,
                    error: Some("Task execution timed out".to_string()),
                    duration_ms: execution_timeout.as_millis() as u64,
                    completed_at: chrono::Utc::now(),
                },
            };

            // Remove from active tasks
            let mut active_tasks = active_tasks.write().await;
            active_tasks.remove(&task_id);
        });

        Ok(handle)
    }

    /// Get queue statistics
    pub async fn queue_stats(&self) -> QueueStats {
        let queue = self.task_queue.read().await;
        queue.stats()
    }

    /// Get active task count
    pub async fn active_task_count(&self) -> usize {
        let active_tasks = self.active_tasks.read().await;
        active_tasks.len()
    }

    /// Cancel a task
    pub async fn cancel_task(&self, task_id: &str) -> GraphResult<bool> {
        let active_tasks = self.active_tasks.read().await;
        if let Some(handle) = active_tasks.get(task_id) {
            let _ = handle.cancel_sender.send(()).await;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Cancel all active tasks
    pub async fn cancel_all_tasks(&self) -> GraphResult<usize> {
        let active_tasks = self.active_tasks.read().await;
        let count = active_tasks.len();
        
        for handle in active_tasks.values() {
            let _ = handle.cancel_sender.send(()).await;
        }
        
        Ok(count)
    }

    /// Get scheduler statistics
    pub async fn stats(&self) -> SchedulerStats {
        let queue_stats = self.queue_stats().await;
        let active_count = self.active_task_count().await;

        SchedulerStats {
            max_concurrent_tasks: self.max_concurrent_tasks,
            active_task_count: active_count,
            queue_stats,
            available_permits: self.semaphore.available_permits(),
        }
    }

    /// Shutdown the scheduler
    pub async fn shutdown(&self) -> GraphResult<()> {
        // Cancel all active tasks
        self.cancel_all_tasks().await?;

        // Clear the queue
        {
            let mut queue = self.task_queue.write().await;
            *queue = TaskQueue::new();
        }

        Ok(())
    }
}

impl<S: GraphState> Default for TaskScheduler<S> {
    fn default() -> Self {
        Self::new()
    }
}

/// Task handle for tracking task execution
#[derive(Debug)]
pub struct TaskHandle<S: GraphState> {
    /// Task ID
    id: String,
    /// Task priority
    priority: TaskPriority,
    /// Start time
    started_at: Instant,
    /// Cancel sender
    cancel_sender: mpsc::Sender<()>,
    /// Phantom data for state type
    _phantom: PhantomData<S>,
}

impl<S: GraphState> TaskHandle<S> {
    /// Get task ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get task priority
    pub fn priority(&self) -> TaskPriority {
        self.priority
    }

    /// Get task age
    pub fn age(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Cancel the task
    pub async fn cancel(&self) -> GraphResult<()> {
        self.cancel_sender.send(()).await.map_err(|_| {
            LangGraphError::runtime("Failed to send cancel signal")
        })?;
        Ok(())
    }
}

/// Scheduler statistics
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    /// Maximum concurrent tasks
    pub max_concurrent_tasks: usize,
    /// Currently active task count
    pub active_task_count: usize,
    /// Queue statistics
    pub queue_stats: QueueStats,
    /// Available permits in semaphore
    pub available_permits: usize,
}

impl SchedulerStats {
    /// Get utilization percentage
    pub fn utilization_percent(&self) -> f64 {
        (self.active_task_count as f64 / self.max_concurrent_tasks as f64) * 100.0
    }

    /// Check if scheduler is at capacity
    pub fn is_at_capacity(&self) -> bool {
        self.active_task_count >= self.max_concurrent_tasks
    }

    /// Check if scheduler is idle
    pub fn is_idle(&self) -> bool {
        self.active_task_count == 0 && self.queue_stats.total_count == 0
    }
}
