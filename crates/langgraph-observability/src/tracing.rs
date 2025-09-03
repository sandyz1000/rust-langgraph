//! Distributed tracing for LangGraph applications

use crate::config::TracingConfig;
use crate::error::ObservabilityResult;
use crate::storage::{SpanStatus, TraceSpan};
use chrono::{DateTime, Utc};
use opentelemetry::trace::TraceContextExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

// TODO: Re-enable OpenTelemetry integration once dependencies are resolved
// Currently disabled due to complex trait compatibility issues

/// Tracing observer that integrates with standard Rust tracing (OpenTelemetry disabled for now)
pub struct TracingObserver {
    config: TracingConfig,
    active_spans: Arc<tokio::sync::RwLock<HashMap<String, TracingSpan>>>,
}

impl TracingObserver {
    /// Create a new tracing observer
    pub async fn new(config: TracingConfig) -> ObservabilityResult<Self> {
        // Initialize tracing subscriber
        Self::init_tracing_subscriber(&config)?;

        Ok(Self {
            config,
            active_spans: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        })
    }

    /// Initialize tracing subscriber for structured logging
    fn init_tracing_subscriber(config: &TracingConfig) -> ObservabilityResult<()> {
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json());

        // TODO: Re-enable OpenTelemetry layer once dependency issues are resolved
        subscriber.init();

        info!("Tracing initialized with service: {}", config.service_name);
        Ok(())
    }

    /// Start a new span for a graph run
    pub async fn start_run_span(
        &self,
        run_id: &str,
        graph_id: &str,
    ) -> ObservabilityResult<String> {
        let span_id = Uuid::new_v4().to_string();

        // Use standard Rust tracing instead of OpenTelemetry for now
        tracing::info!(
            span_id = %span_id,
            run_id = %run_id,
            graph_id = %graph_id,
            "Starting graph run span"
        );

        let tracing_span = TracingSpan {
            id: span_id.clone(),
            start_time: Utc::now(),
            attributes: HashMap::new(),
        };

        let mut spans = self.active_spans.write().await;
        spans.insert(span_id.clone(), tracing_span);

        Ok(span_id)
    }

    /// Start a new span for a node execution
    pub async fn start_node_span(
        &self,
        run_id: &str,
        node_id: &str,
        parent_span_id: Option<&str>,
    ) -> ObservabilityResult<String> {
        let span_id = Uuid::new_v4().to_string();

        // Use standard Rust tracing instead of OpenTelemetry for now
        tracing::info!(
            span_id = %span_id,
            run_id = %run_id,
            node_id = %node_id,
            parent_span_id = ?parent_span_id,
            "Starting node execution span"
        );

        let tracing_span = TracingSpan {
            id: span_id.clone(),
            start_time: Utc::now(),
            attributes: HashMap::new(),
        };

        let mut spans = self.active_spans.write().await;
        spans.insert(span_id.clone(), tracing_span);

        Ok(span_id)
    }

    /// Add an attribute to a span
    pub async fn add_span_attribute(
        &self,
        span_id: &str,
        key: &str,
        value: &str,
    ) -> ObservabilityResult<()> {
        let mut spans = self.active_spans.write().await;
        if let Some(span) = spans.get_mut(span_id) {
            tracing::debug!(
                span_id = %span_id,
                key = %key,
                value = %value,
                "Adding span attribute"
            );
            span.attributes
                .insert(key.to_string(), Value::String(value.to_string()));
        }
        Ok(())
    }

    /// Add an event to a span
    pub async fn add_span_event(
        &self,
        span_id: &str,
        name: &str,
        attributes: HashMap<String, Value>,
    ) -> ObservabilityResult<()> {
        let spans = self.active_spans.read().await;
        if let Some(_span) = spans.get(span_id) {
            tracing::info!(
                span_id = %span_id,
                event_name = %name,
                attributes = ?attributes,
                "Adding span event"
            );
        }
        Ok(())
    }

    /// Finish a span
    pub async fn finish_span(&self, span_id: &str) -> ObservabilityResult<Option<TraceSpan>> {
        let mut spans = self.active_spans.write().await;
        if let Some(span) = spans.remove(span_id) {
            tracing::info!(
                span_id = %span_id,
                duration_ms = ?(Utc::now() - span.start_time).num_milliseconds(),
                "Finishing span"
            );

            let trace_span = TraceSpan {
                span_id: span.id.clone(),
                parent_span_id: None, // TODO: Track parent relationships
                trace_id: span_id.to_string(), // Simplified - should use actual trace ID
                name: format!("span-{}", span.id),
                start_time: span.start_time,
                end_time: Some(Utc::now()),
                attributes: span.attributes,
                events: vec![], // TODO: Track events
                status: SpanStatus::Ok,
            };

            Ok(Some(trace_span))
        } else {
            Ok(None)
        }
    }

    /// Finish a span with an error
    pub async fn finish_span_with_error(
        &self,
        span_id: &str,
        error: &str,
    ) -> ObservabilityResult<Option<TraceSpan>> {
        let mut spans = self.active_spans.write().await;
        if let Some(span) = spans.remove(span_id) {
            tracing::error!(
                span_id = %span_id,
                error = %error,
                duration_ms = ?(Utc::now() - span.start_time).num_milliseconds(),
                "Finishing span with error"
            );

            let trace_span = TraceSpan {
                span_id: span.id.clone(),
                parent_span_id: None,
                trace_id: span_id.to_string(),
                name: format!("span-{}", span.id),
                start_time: span.start_time,
                end_time: Some(Utc::now()),
                attributes: span.attributes,
                events: vec![],
                status: SpanStatus::Error {
                    message: error.to_string(),
                },
            };

            Ok(Some(trace_span))
        } else {
            Ok(None)
        }
    }

    /// Get active span count
    pub async fn active_span_count(&self) -> usize {
        let spans = self.active_spans.read().await;
        spans.len()
    }
}

/// Internal span representation (simplified without OpenTelemetry for now)
struct TracingSpan {
    id: String,
    start_time: DateTime<Utc>,
    attributes: HashMap<String, Value>,
}

/// Trace context that can be passed between nodes
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: Option<String>,
    pub baggage: HashMap<String, String>,
}

impl TraceContext {
    /// Create a new trace context
    pub fn new(trace_id: String) -> Self {
        Self {
            trace_id,
            span_id: None,
            baggage: HashMap::new(),
        }
    }

    /// Set the current span ID
    pub fn with_span(mut self, span_id: String) -> Self {
        self.span_id = Some(span_id);
        self
    }

    /// Add baggage to the context
    pub fn with_baggage(mut self, key: String, value: String) -> Self {
        self.baggage.insert(key, value);
        self
    }
}

/// Macro for instrumenting functions with tracing
#[macro_export]
macro_rules! trace_fn {
    ($tracer:expr, $name:expr, $($key:expr => $value:expr),*) => {{
        let span = $tracer.span_builder($name)
            .with_attributes(vec![
                $(KeyValue::new($key, $value.to_string())),*
            ])
            .start(&$tracer);
        let _guard = span.enter();
    }};
}

/// Helper function to extract trace context from OpenTelemetry context
pub fn extract_trace_context() -> TraceContext {
    let context = opentelemetry::Context::current();
    let binding = context.span();
    let span_context = binding.span_context();

    TraceContext {
        trace_id: span_context.trace_id().to_string(),
        span_id: Some(span_context.span_id().to_string()),
        baggage: HashMap::new(), // TODO: Extract actual baggage
    }
}
