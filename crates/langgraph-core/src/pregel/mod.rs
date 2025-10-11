//! Pregel execution engine for distributed graph computation

mod engine;
mod scheduler;
mod task;

pub use engine::PregelEngine;
pub use scheduler::TaskScheduler;
pub use task::{PregelTask, TaskResult, TaskStatus};
