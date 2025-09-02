# LangGraph Observability 🔍

A comprehensive observability and debugging toolkit for LangGraph applications, providing functionality similar to LangSmith but designed specifically for the Rust ecosystem.

## Features

### 🔍 **Distributed Tracing**
- OpenTelemetry integration for distributed tracing
- Jaeger and OTLP export support
- Automatic span creation for graph runs and node executions
- Correlation IDs for tracking requests across services

### 📊 **Metrics Collection**
- Prometheus-compatible metrics
- Real-time performance monitoring
- Custom metrics support
- System resource tracking

### 🎯 **Debug Dashboard**
- Web-based UI for monitoring and debugging
- Real-time event streaming via WebSocket
- Interactive run inspection
- Prompt analysis and optimization

### 🧠 **Prompt Analysis**
- Automatic prompt quality scoring
- Token efficiency analysis
- Injection risk detection
- Cost estimation and optimization suggestions

### 💾 **Flexible Storage**
- In-memory storage for development
- SQLite for local persistence
- PostgreSQL for production deployments
- Pluggable storage backends

### ⚡ **Real-time Events**
- Event bus for real-time monitoring
- WebSocket streaming for live updates
- Customizable event filters
- Automatic event persistence

## Quick Start

### 1. Add to Dependencies

```toml
[dependencies]
langgraph-observability = { path = "crates/langgraph-observability" }
```

### 2. Basic Setup

```rust
use langgraph_observability::{Observability, ObservabilityConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure observability
    let config = ObservabilityConfig::builder()
        .with_tracing(true)
        .with_metrics(true)
        .with_dashboard(true)
        .build();
    
    // Initialize observability system
    let observability = Observability::new(config).await?;
    
    // Start dashboard
    observability.start_dashboard("127.0.0.1:3000").await?;
    
    // Create graph observer
    let observer = observability.create_graph_observer();
    
    // Use with your LangGraph...
    
    Ok(())
}
```

### 3. Run the Example

```bash
cd rust-langgraph
cargo run --example observability_demo
```

Then open http://localhost:3000 in your browser to explore the dashboard.

## Configuration

### Tracing Configuration

```rust
let tracing_config = TracingConfig {
    enabled: true,
    jaeger_endpoint: Some("http://localhost:14268/api/traces".to_string()),
    otlp_endpoint: Some("http://localhost:4317".to_string()),
    service_name: "my-langgraph-app".to_string(),
    sample_rate: 1.0, // Sample 100% of traces
    default_attributes: HashMap::new(),
};
```

### Metrics Configuration

```rust
let metrics_config = MetricsConfig {
    enabled: true,
    prometheus_path: "/metrics".to_string(),
    collection_interval_seconds: 30,
    custom_metrics: vec![],
};
```

### Dashboard Configuration

```rust
let dashboard_config = DashboardConfig {
    enabled: true,
    host: "127.0.0.1".to_string(),
    port: 3000,
    static_assets_path: Some("./static".to_string()),
    websocket_enabled: true,
    auth: None, // Optional authentication
};
```

### Storage Configuration

```rust
// In-memory (development)
let storage = StorageConfig::InMemory;

// SQLite (local persistence)
let storage = StorageConfig::Sqlite { 
    path: "./observability.db".to_string() 
};

// PostgreSQL (production)
let storage = StorageConfig::Postgres { 
    url: "postgresql://user:pass@localhost/observability".to_string() 
};
```

## Dashboard Features

### 🏠 **Home Dashboard**
- Real-time metrics overview
- Active runs monitoring
- Success rate tracking
- Live event stream

### 🏃 **Runs View**
- List all graph executions
- Filter by status, graph type, time range
- Detailed run inspection
- Performance analysis

### 📊 **Metrics View**
- Performance dashboards
- Resource utilization
- Custom metrics visualization
- Historical trends

### 💬 **Prompts View**
- Prompt analysis and debugging
- Token usage optimization
- Quality scoring
- A/B testing support

## Prompt Analysis Features

### Quality Scoring
Automatically analyzes prompts and provides quality scores based on:
- Token efficiency
- Output relevance
- Prompt clarity
- Pattern detection

### Token Optimization
- Input/output token ratios
- Cost estimation
- Efficiency recommendations
- Usage patterns

### Risk Detection
- Prompt injection detection
- Safety analysis
- Pattern recognition
- Security recommendations

### Pattern Recognition
Detects common prompt patterns:
- Few-shot examples
- Chain-of-thought prompting
- Role-playing instructions
- Structured output requests

## API Endpoints

### REST API

```
GET  /api/runs                 - List graph runs
GET  /api/runs/:id             - Get specific run
GET  /api/runs/:id/spans       - Get run traces
GET  /api/runs/:id/prompts     - Get run prompts
GET  /api/metrics              - Get metrics data
GET  /api/health               - Health check
```

### WebSocket Events

```
ws://localhost:3000/ws         - Real-time event stream
```

## Event Types

The observability system emits various events:

```rust
pub enum ObservabilityEvent {
    RunStarted { run_id: String },
    RunComplete { run_id: String, duration_ms: u64 },
    RunFailed { run_id: String, error: String },
    NodeStarted { run_id: String, node_id: String, timestamp: DateTime<Utc> },
    NodeCompleted { run_id: String, node_id: String, duration_ms: u64, timestamp: DateTime<Utc> },
    NodeFailed { run_id: String, node_id: String, error: String, timestamp: DateTime<Utc> },
    PromptExecuted { run_id: String, node_id: String, prompt_id: String, model: String, ... },
    // ... more event types
}
```

## Integration Examples

### With Existing LangGraph

```rust
use langgraph_observability::GraphObserver;

// Create observer
let observer = observability.create_graph_observer();

// Start run observation
let run_id = observer.start_run("my-graph".to_string()).await?;

// Execute your graph with context
let mut config = GraphConfig::new();
config.set_config("run_id", run_id.clone())?;

let result = graph.invoke(initial_state, config).await?;

// Observe completion
observer.observe_run(&run).await?;
```

### Custom Metrics

```rust
use langgraph_observability::{MetricsCollector, CustomMetric, MetricType};

let metrics = observability.metrics_collector();

// Record custom metric
metrics.record_custom_metric(CustomMetric {
    name: "custom_operation_duration".to_string(),
    metric_type: MetricType::Histogram,
    labels: HashMap::from([
        ("operation".to_string(), "data_processing".to_string()),
    ]),
    value: 1.23,
}).await?;
```

### Event Streaming

```rust
use langgraph_observability::{EventBus, EventFilter};

// Subscribe to events
observability.event_bus().subscribe(
    "my_subscriber".to_string(),
    |event| {
        println!("Received event: {:?}", event);
        Ok(())
    }
).await?;

// Create filtered stream
let mut stream = EventStream::new(observability.event_bus().receiver())
    .with_filter(EventFilter::RunId("specific-run-id".to_string()));

while let Ok(event) = stream.next().await {
    // Handle filtered events
}
```

## Production Deployment

### Docker Compose Example

```yaml
version: '3.8'
services:
  langgraph-app:
    build: .
    environment:
      - OBSERVABILITY_STORAGE=postgres
      - DATABASE_URL=postgresql://user:pass@postgres/observability
      - JAEGER_ENDPOINT=http://jaeger:14268/api/traces
    ports:
      - "3000:3000"
    depends_on:
      - postgres
      - jaeger

  postgres:
    image: postgres:13
    environment:
      POSTGRES_DB: observability
      POSTGRES_USER: user
      POSTGRES_PASSWORD: pass

  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"
      - "14268:14268"

  prometheus:
    image: prom/prometheus
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: langgraph-observability
spec:
  replicas: 3
  selector:
    matchLabels:
      app: langgraph-observability
  template:
    metadata:
      labels:
        app: langgraph-observability
    spec:
      containers:
      - name: app
        image: langgraph-app:latest
        ports:
        - containerPort: 3000
        env:
        - name: OBSERVABILITY_STORAGE
          value: "postgres"
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-secret
              key: url
```

## Architecture

The observability system consists of several key components:

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   LangGraph     │────│   Graph Observer │────│   Event Bus     │
│   Application   │    │                  │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                │                        │
                                ▼                        ▼
                       ┌──────────────────┐    ┌─────────────────┐
                       │   Storage Layer  │    │   Dashboard     │
                       │   (SQLite/PG)    │    │   Web Server    │
                       └──────────────────┘    └─────────────────┘
                                │                        │
                                ▼                        ▼
                       ┌──────────────────┐    ┌─────────────────┐
                       │   Metrics        │    │   WebSocket     │
                       │   Collector      │    │   Events        │
                       └──────────────────┘    └─────────────────┘
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Submit a pull request

## License

MIT License - see LICENSE file for details.

## Comparison with LangSmith

| Feature | LangSmith | LangGraph Observability |
|---------|-----------|------------------------|
| Tracing | ✅ | ✅ OpenTelemetry |
| Metrics | ✅ | ✅ Prometheus |
| Dashboard | ✅ | ✅ Web UI |
| Prompt Analysis | ✅ | ✅ Built-in |
| Real-time Events | ✅ | ✅ WebSocket |
| Storage | ☁️ Cloud | 🏠 Self-hosted |
| Language | Python | 🦀 Rust |
| Cost | 💰 Paid | 🆓 Open Source |

This observability toolkit provides similar functionality to LangSmith but is:
- **Self-hosted**: Full control over your data
- **Open source**: No vendor lock-in
- **Rust-native**: Type-safe and performant
- **Extensible**: Pluggable architecture
