//! Configuration for LangGraph Observability

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main configuration for the observability system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Tracing configuration
    pub tracing: TracingConfig,
    /// Metrics configuration
    pub metrics: MetricsConfig,
    /// Dashboard configuration
    pub dashboard: DashboardConfig,
    /// Storage configuration
    pub storage: StorageConfig,
    /// Event bus configuration
    pub events: EventConfig,
    /// Prompt analysis configuration
    pub prompt_analysis: PromptAnalysisConfig,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            tracing: TracingConfig::default(),
            metrics: MetricsConfig::default(),
            dashboard: DashboardConfig::default(),
            storage: StorageConfig::default(),
            events: EventConfig::default(),
            prompt_analysis: PromptAnalysisConfig::default(),
        }
    }
}

impl ObservabilityConfig {
    /// Create a new configuration builder
    pub fn builder() -> ObservabilityConfigBuilder {
        ObservabilityConfigBuilder::default()
    }
}

/// Builder for observability configuration
#[derive(Debug, Default)]
pub struct ObservabilityConfigBuilder {
    tracing: Option<TracingConfig>,
    metrics: Option<MetricsConfig>,
    dashboard: Option<DashboardConfig>,
    storage: Option<StorageConfig>,
    events: Option<EventConfig>,
    prompt_analysis: Option<PromptAnalysisConfig>,
}

impl ObservabilityConfigBuilder {
    /// Enable/disable tracing
    pub fn with_tracing(mut self, enabled: bool) -> Self {
        if let Some(ref mut config) = self.tracing {
            config.enabled = enabled;
        } else {
            self.tracing = Some(TracingConfig {
                enabled,
                ..Default::default()
            });
        }
        self
    }

    /// Configure tracing with custom config
    pub fn with_tracing_config(mut self, config: TracingConfig) -> Self {
        self.tracing = Some(config);
        self
    }

    /// Enable/disable metrics
    pub fn with_metrics(mut self, enabled: bool) -> Self {
        if let Some(ref mut config) = self.metrics {
            config.enabled = enabled;
        } else {
            self.metrics = Some(MetricsConfig {
                enabled,
                ..Default::default()
            });
        }
        self
    }

    /// Configure metrics with custom config
    pub fn with_metrics_config(mut self, config: MetricsConfig) -> Self {
        self.metrics = Some(config);
        self
    }

    /// Enable/disable dashboard
    pub fn with_dashboard(mut self, enabled: bool) -> Self {
        if let Some(ref mut config) = self.dashboard {
            config.enabled = enabled;
        } else {
            self.dashboard = Some(DashboardConfig {
                enabled,
                ..Default::default()
            });
        }
        self
    }

    /// Configure dashboard with custom config
    pub fn with_dashboard_config(mut self, config: DashboardConfig) -> Self {
        self.dashboard = Some(config);
        self
    }

    /// Configure storage
    pub fn with_storage(mut self, config: StorageConfig) -> Self {
        self.storage = Some(config);
        self
    }

    /// Build the configuration
    pub fn build(self) -> ObservabilityConfig {
        ObservabilityConfig {
            tracing: self.tracing.unwrap_or_default(),
            metrics: self.metrics.unwrap_or_default(),
            dashboard: self.dashboard.unwrap_or_default(),
            storage: self.storage.unwrap_or_default(),
            events: self.events.unwrap_or_default(),
            prompt_analysis: self.prompt_analysis.unwrap_or_default(),
        }
    }
}

/// Configuration for distributed tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Whether tracing is enabled
    pub enabled: bool,
    /// Jaeger endpoint for exporting traces
    pub jaeger_endpoint: Option<String>,
    /// OTLP endpoint for exporting traces
    pub otlp_endpoint: Option<String>,
    /// Service name for traces
    pub service_name: String,
    /// Sample rate (0.0 to 1.0)
    pub sample_rate: f64,
    /// Additional attributes to add to all spans
    pub default_attributes: HashMap<String, String>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            jaeger_endpoint: None,
            otlp_endpoint: None,
            service_name: "langgraph-app".to_string(),
            sample_rate: 1.0,
            default_attributes: HashMap::new(),
        }
    }
}

/// Configuration for metrics collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Whether metrics collection is enabled
    pub enabled: bool,
    /// Prometheus metrics endpoint path
    pub prometheus_path: String,
    /// Metrics collection interval in seconds
    pub collection_interval_seconds: u64,
    /// Custom metrics to collect
    pub custom_metrics: Vec<String>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prometheus_path: "/metrics".to_string(),
            collection_interval_seconds: 30,
            custom_metrics: vec![],
        }
    }
}

/// Configuration for the debug dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    /// Whether the dashboard is enabled
    pub enabled: bool,
    /// Host to bind the dashboard server to
    pub host: String,
    /// Port to bind the dashboard server to
    pub port: u16,
    /// Path to static assets for the dashboard
    pub static_assets_path: Option<String>,
    /// Whether to enable WebSocket for real-time updates
    pub websocket_enabled: bool,
    /// Authentication configuration
    pub auth: Option<AuthConfig>,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 3000,
            static_assets_path: None,
            websocket_enabled: true,
            auth: None,
        }
    }
}

/// Authentication configuration for the dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Whether authentication is enabled
    pub enabled: bool,
    /// Authentication method
    pub method: AuthMethod,
    /// API key for simple authentication
    pub api_key: Option<String>,
}

/// Authentication methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    /// No authentication
    None,
    /// Simple API key authentication
    ApiKey,
    /// JWT token authentication
    Jwt,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageConfig {
    /// In-memory storage (not persistent)
    InMemory,
    /// SQLite file storage
    Sqlite { path: String },
    /// PostgreSQL database storage
    Postgres { url: String },
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self::InMemory
    }
}

/// Event bus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConfig {
    /// Buffer size for events
    pub buffer_size: usize,
    /// Whether to persist events to storage
    pub persist_events: bool,
    /// Event retention period in hours
    pub retention_hours: u64,
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1000,
            persist_events: true,
            retention_hours: 24,
        }
    }
}

/// Configuration for prompt analysis features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptAnalysisConfig {
    /// Whether prompt analysis is enabled
    pub enabled: bool,
    /// Whether to capture prompt inputs and outputs
    pub capture_prompts: bool,
    /// Whether to analyze token usage
    pub analyze_tokens: bool,
    /// Whether to detect prompt injection attempts
    pub detect_injection: bool,
    /// Maximum prompt length to store
    pub max_prompt_length: usize,
}

impl Default for PromptAnalysisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            capture_prompts: true,
            analyze_tokens: true,
            detect_injection: false,
            max_prompt_length: 10000,
        }
    }
}
