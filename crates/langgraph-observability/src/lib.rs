//! LangGraph Observability - A comprehensive observability and debugging toolkit
//!
//! This crate provides observability features similar to LangSmith for debugging
//! and monitoring LangGraph applications. It includes:
//!
//! - **Tracing**: Distributed tracing with OpenTelemetry support
//! - **Metrics**: Performance and usage metrics with Prometheus integration
//! - **Logging**: Structured logging with correlation IDs
//! - **Debug Dashboard**: Web-based UI for debugging prompts and graph execution
//! - **Event Streaming**: Real-time monitoring of graph execution
//! - **Prompt Analysis**: Detailed analysis of LLM interactions and prompts
//!
//! # Quick Start
//!
//! ```rust
//! use langgraph_observability::{Observability, ObservabilityConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize observability
//!     let config = ObservabilityConfig::builder()
//!         .with_tracing(true)
//!         .with_metrics(true)
//!         .with_dashboard(true)
//!         .build();
//!     
//!     let observability = Observability::new(config).await?;
//!     
//!     // Start the dashboard server
//!     observability.start_dashboard("127.0.0.1:3000").await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod dashboard;
pub mod error;
pub mod events;
pub mod metrics;
pub mod prompt_analysis;
pub mod storage;
#[cfg(feature = "tracing")]
pub mod tracing;

// Frontend module for Dioxus components
#[cfg(feature = "frontend")]
pub mod frontend;

pub use config::*;
pub use dashboard::Dashboard;
pub use error::*;
pub use events::*;
pub use metrics::MetricsCollector;
pub use prompt_analysis::*;
pub use storage::*;
#[cfg(feature = "tracing")]
pub use tracing::TracingObserver;

use std::sync::Arc;
use tokio::sync::RwLock;

/// Main observability system that coordinates all observability components
pub struct Observability {
    config: ObservabilityConfig,
    #[cfg(feature = "tracing")]
    tracing_observer: Option<TracingObserver>,
    #[cfg(feature = "metrics")]
    metrics_collector: Option<MetricsCollector>,
    dashboard: Option<Dashboard>,
    storage: Arc<RwLock<dyn ObservabilityStorage>>,
    event_bus: EventBus,
}

impl Observability {
    /// Create a new observability system with the given configuration
    pub async fn new(config: ObservabilityConfig) -> ObservabilityResult<Self> {
        // Initialize storage
        let storage: Arc<RwLock<dyn ObservabilityStorage>> = match config.storage.clone() {
            StorageConfig::InMemory => Arc::new(RwLock::new(InMemoryStorage::new())),
            StorageConfig::Sqlite { path } => {
                Arc::new(RwLock::new(SqliteStorage::new(&path).await?))
            }
            StorageConfig::Postgres { url } => {
                Arc::new(RwLock::new(PostgresStorage::new(&url).await?))
            }
        };

        // Initialize event bus
        let event_bus = EventBus::new();

        // Initialize tracing observer if enabled
        #[cfg(feature = "tracing")]
        let tracing_observer = if config.tracing.enabled {
            Some(TracingObserver::new(config.tracing.clone()).await?)
        } else {
            None
        };

        // Initialize metrics collector if enabled
        #[cfg(feature = "metrics")]
        let metrics_collector = if config.metrics.enabled {
            Some(MetricsCollector::new(config.metrics.clone()).await?)
        } else {
            None
        };

        // Initialize dashboard if enabled
        let dashboard = if config.dashboard.enabled {
            Some(Dashboard::new(
                config.dashboard.clone(),
                storage.clone(),
                event_bus.clone(),
            ))
        } else {
            None
        };

        Ok(Self {
            config,
            #[cfg(feature = "tracing")]
            tracing_observer,
            #[cfg(feature = "metrics")]
            metrics_collector,
            dashboard,
            storage,
            event_bus,
        })
    }

    /// Start the dashboard server
    pub async fn start_dashboard(&self, addr: &str) -> ObservabilityResult<()> {
        if let Some(dashboard) = &self.dashboard {
            dashboard.start(addr).await?;
        }
        Ok(())
    }

    /// Get the event bus for publishing events
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// Get the storage backend
    pub fn storage(&self) -> Arc<RwLock<dyn ObservabilityStorage>> {
        self.storage.clone()
    }

    /// Create a graph observer that can be attached to a LangGraph instance
    pub fn create_graph_observer(&self) -> GraphObserver {
        GraphObserver::new(
            self.storage.clone(),
            self.event_bus.clone(),
            self.config.clone(),
        )
    }

    /// Shutdown the observability system
    pub async fn shutdown(&self) -> ObservabilityResult<()> {
        if let Some(dashboard) = &self.dashboard {
            dashboard.shutdown().await?;
        }
        Ok(())
    }
}

/// Graph observer that can be attached to LangGraph instances to collect observability data
pub struct GraphObserver {
    storage: Arc<RwLock<dyn ObservabilityStorage>>,
    event_bus: EventBus,
    config: ObservabilityConfig,
}

impl GraphObserver {
    fn new(
        storage: Arc<RwLock<dyn ObservabilityStorage>>,
        event_bus: EventBus,
        config: ObservabilityConfig,
    ) -> Self {
        Self {
            storage,
            event_bus,
            config,
        }
    }

    /// Observe a graph execution run
    pub async fn observe_run(&self, run: &GraphRun<serde_json::Value>) -> ObservabilityResult<()> {
        // Store the run
        let mut storage = self.storage.write().await;
        storage.store_run(run).await?;

        // Publish event
        self.event_bus
            .publish(ObservabilityEvent::RunComplete {
                run_id: run.id.clone(),
                duration_ms: run.duration_ms(),
            })
            .await?;

        Ok(())
    }

    /// Start observing a graph execution
    pub async fn start_run(&self, graph_id: String) -> ObservabilityResult<String> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let run = GraphRun::new(run_id.clone(), graph_id);

        let mut storage = self.storage.write().await;
        storage.store_run(&run).await?;

        self.event_bus
            .publish(ObservabilityEvent::RunStarted {
                run_id: run_id.clone(),
            })
            .await?;

        Ok(run_id)
    }
}
