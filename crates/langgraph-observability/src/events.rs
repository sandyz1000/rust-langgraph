//! Event system for real-time observability

use crate::error::{ObservabilityError, ObservabilityResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Event bus for distributing observability events
#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<ObservabilityEvent>,
    subscribers: Arc<RwLock<HashMap<String, EventSubscriber>>>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self {
            sender,
            subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Publish an event to all subscribers
    pub async fn publish(&self, event: ObservabilityEvent) -> ObservabilityResult<()> {
        self.sender
            .send(event)
            .map_err(|e| ObservabilityError::EventBus(e.to_string()))?;
        Ok(())
    }

    /// Subscribe to events with a callback
    pub async fn subscribe<F>(&self, id: String, callback: F) -> ObservabilityResult<()>
    where
        F: Fn(ObservabilityEvent) -> Result<(), ObservabilityError> + Send + Sync + 'static,
    {
        let subscriber = EventSubscriber {
            id: id.clone(),
            receiver: self.sender.subscribe(),
            callback: Arc::new(callback),
        };

        let mut subscribers = self.subscribers.write().await;
        subscribers.insert(id, subscriber);
        Ok(())
    }

    /// Unsubscribe from events
    pub async fn unsubscribe(&self, id: &str) -> ObservabilityResult<()> {
        let mut subscribers = self.subscribers.write().await;
        subscribers.remove(id);
        Ok(())
    }

    /// Get a receiver for events
    pub fn receiver(&self) -> broadcast::Receiver<ObservabilityEvent> {
        self.sender.subscribe()
    }
}

/// Event subscriber
pub struct EventSubscriber {
    pub id: String,
    pub receiver: broadcast::Receiver<ObservabilityEvent>,
    pub callback: Arc<dyn Fn(ObservabilityEvent) -> Result<(), ObservabilityError> + Send + Sync>,
}

impl std::fmt::Debug for EventSubscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventSubscriber")
            .field("id", &self.id)
            .field("receiver", &"<receiver>")
            .field("callback", &"<callback>")
            .finish()
    }
}

/// Events emitted by the observability system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObservabilityEvent {
    /// Graph run started
    RunStarted { run_id: String },
    /// Graph run completed
    RunComplete { run_id: String, duration_ms: u64 },
    /// Graph run failed
    RunFailed { run_id: String, error: String },
    /// Node execution started
    NodeStarted {
        run_id: String,
        node_id: String,
        timestamp: DateTime<Utc>,
    },
    /// Node execution completed
    NodeCompleted {
        run_id: String,
        node_id: String,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// Node execution failed
    NodeFailed {
        run_id: String,
        node_id: String,
        error: String,
        timestamp: DateTime<Utc>,
    },
    /// State updated
    StateUpdated {
        run_id: String,
        node_id: Option<String>,
        state_diff: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
    /// Checkpoint created
    CheckpointCreated {
        run_id: String,
        checkpoint_id: String,
        step: u32,
        timestamp: DateTime<Utc>,
    },
    /// Prompt executed
    PromptExecuted {
        run_id: String,
        node_id: String,
        prompt_id: String,
        model: String,
        input_tokens: u32,
        output_tokens: u32,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// Custom event
    Custom {
        event_type: String,
        data: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
}

impl ObservabilityEvent {
    /// Get the run ID associated with this event
    pub fn run_id(&self) -> Option<&str> {
        match self {
            Self::RunStarted { run_id }
            | Self::RunComplete { run_id, .. }
            | Self::RunFailed { run_id, .. }
            | Self::NodeStarted { run_id, .. }
            | Self::NodeCompleted { run_id, .. }
            | Self::NodeFailed { run_id, .. }
            | Self::StateUpdated { run_id, .. }
            | Self::CheckpointCreated { run_id, .. }
            | Self::PromptExecuted { run_id, .. } => Some(run_id),
            Self::Custom { .. } => None,
        }
    }

    /// Get the timestamp of this event
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::RunStarted { .. } => Utc::now(),
            Self::RunComplete { .. } => Utc::now(),
            Self::RunFailed { .. } => Utc::now(),
            Self::NodeStarted { timestamp, .. }
            | Self::NodeCompleted { timestamp, .. }
            | Self::NodeFailed { timestamp, .. }
            | Self::StateUpdated { timestamp, .. }
            | Self::CheckpointCreated { timestamp, .. }
            | Self::PromptExecuted { timestamp, .. }
            | Self::Custom { timestamp, .. } => *timestamp,
        }
    }

    /// Create a new custom event
    pub fn custom(event_type: String, data: serde_json::Value) -> Self {
        Self::Custom {
            event_type,
            data,
            timestamp: Utc::now(),
        }
    }
}

/// Real-time event stream for WebSocket connections
pub struct EventStream {
    receiver: broadcast::Receiver<ObservabilityEvent>,
    filters: Vec<EventFilter>,
}

impl EventStream {
    /// Create a new event stream
    pub fn new(receiver: broadcast::Receiver<ObservabilityEvent>) -> Self {
        Self {
            receiver,
            filters: vec![],
        }
    }

    /// Add a filter to the stream
    pub fn with_filter(mut self, filter: EventFilter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Get the next event that matches all filters
    pub async fn next(&mut self) -> ObservabilityResult<ObservabilityEvent> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    if self.filters.iter().all(|filter| filter.matches(&event)) {
                        return Ok(event);
                    }
                }
                Err(e) => {
                    return Err(ObservabilityError::EventBus(e.to_string()));
                }
            }
        }
    }
}

/// Filter for events
#[derive(Clone)]
pub enum EventFilter {
    /// Filter by run ID
    RunId(String),
    /// Filter by event type
    EventType(String),
    /// Filter by node ID
    NodeId(String),
    /// Custom filter function
    Custom(Arc<dyn Fn(&ObservabilityEvent) -> bool + Send + Sync>),
}

impl std::fmt::Debug for EventFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RunId(run_id) => f.debug_tuple("RunId").field(run_id).finish(),
            Self::EventType(event_type) => f.debug_tuple("EventType").field(event_type).finish(),
            Self::NodeId(node_id) => f.debug_tuple("NodeId").field(node_id).finish(),
            Self::Custom(_) => f.debug_tuple("Custom").field(&"<function>").finish(),
        }
    }
}

impl EventFilter {
    /// Check if an event matches this filter
    pub fn matches(&self, event: &ObservabilityEvent) -> bool {
        match self {
            Self::RunId(run_id) => event.run_id() == Some(run_id),
            Self::EventType(event_type) => {
                let event_type_name = match event {
                    ObservabilityEvent::RunStarted { .. } => "RunStarted",
                    ObservabilityEvent::RunComplete { .. } => "RunComplete",
                    ObservabilityEvent::RunFailed { .. } => "RunFailed",
                    ObservabilityEvent::NodeStarted { .. } => "NodeStarted",
                    ObservabilityEvent::NodeCompleted { .. } => "NodeCompleted",
                    ObservabilityEvent::NodeFailed { .. } => "NodeFailed",
                    ObservabilityEvent::StateUpdated { .. } => "StateUpdated",
                    ObservabilityEvent::CheckpointCreated { .. } => "CheckpointCreated",
                    ObservabilityEvent::PromptExecuted { .. } => "PromptExecuted",
                    ObservabilityEvent::Custom { event_type, .. } => event_type,
                };
                event_type_name == event_type
            }
            Self::NodeId(node_id) => match event {
                ObservabilityEvent::NodeStarted { node_id: n, .. }
                | ObservabilityEvent::NodeCompleted { node_id: n, .. }
                | ObservabilityEvent::NodeFailed { node_id: n, .. }
                | ObservabilityEvent::PromptExecuted { node_id: n, .. } => n == node_id,
                ObservabilityEvent::StateUpdated {
                    node_id: Some(n), ..
                } => n == node_id,
                _ => false,
            },
            Self::Custom(filter_fn) => filter_fn(event),
        }
    }
}
