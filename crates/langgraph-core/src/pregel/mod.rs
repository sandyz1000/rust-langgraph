//! Pregel execution engine for distributed graph computation

mod engine;
mod task;
mod scheduler;

pub use engine::PregelEngine;
pub use task::{PregelTask, TaskStatus, TaskResult};
pub use scheduler::TaskScheduler;
