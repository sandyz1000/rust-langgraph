//! Distributed tracing for LangGraph applications

use crate::config::TracingConfig;
use crate::error::{ObservabilityError, ObservabilityResult};
use crate::storage::{SpanEvent, SpanStatus, TraceSpan};
use chrono::{DateTime, Utc};
#[cfg(feature = "tracing")]
use opentelemetry::{
    global,
    sdk::{
        trace::{self, Tracer},
        Resource,
    },
    trace::{Span, SpanKind, Status, TraceContextExt, Tracer as _},
    Context, KeyValue,
};
#[cfg(feature = "tracing")]
use opentelemetry_jaeger::JaegerPipeline;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

/// Tracing observer that integrates with OpenTelemetry
pub struct TracingObserver {
    config: TracingConfig,
    tracer: Arc<dyn opentelemetry::trace::Tracer + Send + Sync>,
    active_spans: Arc<tokio::sync::RwLock<HashMap<String, TracingSpan>>>,
}

impl TracingObserver {
    /// Create a new tracing observer
    pub async fn new(config: TracingConfig) -> ObservabilityResult<Self> {
        // Initialize OpenTelemetry tracer
        let tracer = Self::init_tracer(&config).await?;

        // Initialize tracing subscriber
        Self::init_tracing_subscriber(&config)?;

        Ok(Self {
            config,
            tracer,
            active_spans: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        })
    }

    /// Initialize OpenTelemetry tracer
    async fn init_tracer(
        config: &TracingConfig,
    ) -> ObservabilityResult<Arc<dyn opentelemetry::trace::Tracer + Send + Sync>> {
        let resource = Resource::new(vec![
            KeyValue::new("service.name", config.service_name.clone()),
            KeyValue::new("service.version", "1.0.0"),
        ]);

        // Create tracer pipeline
        let mut pipeline = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_resource(resource)
            .with_sampler(trace::Sampler::TraceIdRatioBased(config.sample_rate));

        // Configure exporter based on config
        if let Some(ref jaeger_endpoint) = config.jaeger_endpoint {
            info!("Configuring Jaeger tracing exporter: {}", jaeger_endpoint);
            // Configure Jaeger exporter (simplified - would need full Jaeger setup)
        }

        if let Some(ref otlp_endpoint) = config.otlp_endpoint {
            info!("Configuring OTLP tracing exporter: {}", otlp_endpoint);
            // Configure OTLP exporter
        }

        let tracer_provider = pipeline.build();
        global::set_tracer_provider(tracer_provider.clone());

        let tracer = tracer_provider.tracer("langgraph-observability");
        Ok(Arc::new(tracer))
    }

    /// Initialize tracing subscriber for structured logging
    fn init_tracing_subscriber(config: &TracingConfig) -> ObservabilityResult<()> {
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json());

        // Add OpenTelemetry layer if tracing is enabled
        if config.enabled {
            let telemetry_layer = tracing_opentelemetry::layer()
                .with_tracer(global::tracer("langgraph-observability"));
            subscriber.with(telemetry_layer).init();
        } else {
            subscriber.init();
        }

        Ok(())
    }

    /// Start a new span for a graph run
    pub async fn start_run_span(&self, run_id: &str, graph_id: &str) -> ObservabilityResult<String> {
        let span_id = Uuid::new_v4().to_string();
        let span = self
            .tracer
            .span_builder(format!("graph_run:{}", graph_id))
            .with_kind(SpanKind::Server)
            .with_attributes(vec![
                KeyValue::new("run.id", run_id.to_string()),
                KeyValue::new("graph.id", graph_id.to_string()),
            ])
            .start(&self.tracer);

        let tracing_span = TracingSpan {
            id: span_id.clone(),
            otel_span: Box::new(span),
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

        // Get parent context if available
        let parent_context = if let Some(parent_id) = parent_span_id {
            let spans = self.active_spans.read().await;
            spans
                .get(parent_id)
                .map(|parent| Context::current_with_span(&*parent.otel_span))
                .unwrap_or_else(Context::current)
        } else {
            Context::current()
        };

        let span = self
            .tracer
            .span_builder(format!("node:{}", node_id))
            .with_kind(SpanKind::Internal)
            .with_attributes(vec![
                KeyValue::new("run.id", run_id.to_string()),
                KeyValue::new("node.id", node_id.to_string()),
            ])
            .start_with_context(&self.tracer, &parent_context);

        let tracing_span = TracingSpan {
            id: span_id.clone(),
            otel_span: Box::new(span),
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
            span.otel_span
                .set_attribute(KeyValue::new(key.to_string(), value.to_string()));
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
        if let Some(span) = spans.get(span_id) {
            let otel_attributes: Vec<KeyValue> = attributes
                .iter()
                .map(|(k, v)| KeyValue::new(k.clone(), v.to_string()))
                .collect();

            span.otel_span.add_event(name.to_string(), otel_attributes);
        }
        Ok(())
    }

    /// Finish a span
    pub async fn finish_span(&self, span_id: &str) -> ObservabilityResult<Option<TraceSpan>> {
        let mut spans = self.active_spans.write().await;
        if let Some(mut span) = spans.remove(span_id) {
            span.otel_span.end();

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
        if let Some(mut span) = spans.remove(span_id) {
            span.otel_span.set_status(Status::error(error.to_string()));
            span.otel_span.end();

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

/// Internal span representation
struct TracingSpan {
    id: String,
    otel_span: Box<dyn opentelemetry::trace::Span + Send + Sync>,
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
    let context = Context::current();
    let span_context = context.span().span_context();

    TraceContext {
        trace_id: span_context.trace_id().to_string(),
        span_id: Some(span_context.span_id().to_string()),
        baggage: HashMap::new(), // TODO: Extract actual baggage
    }
}
