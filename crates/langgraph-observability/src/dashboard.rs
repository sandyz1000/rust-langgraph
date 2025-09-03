//! Web dashboard for LangGraph observability and debugging

use crate::config::DashboardConfig;
use crate::error::{ObservabilityError, ObservabilityResult};
use crate::events::{EventBus, EventStream};
#[cfg(feature = "frontend")]
use crate::frontend;
use crate::storage::{ObservabilityStorage, RunFilters};
use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
#[cfg(feature = "frontend")]
use dioxus_ssr::render;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

/// Dashboard server for observability UI
pub struct Dashboard {
    config: DashboardConfig,
    storage: Arc<RwLock<dyn ObservabilityStorage>>,
    event_bus: EventBus,
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Dashboard {
    /// Create a new dashboard
    pub fn new(
        config: DashboardConfig,
        storage: Arc<RwLock<dyn ObservabilityStorage>>,
        event_bus: EventBus,
    ) -> Self {
        Self {
            config,
            storage,
            event_bus,
            server_handle: None,
        }
    }

    /// Start the dashboard server
    pub async fn start(&self, addr: &str) -> ObservabilityResult<()> {
        let app_state = AppState {
            storage: self.storage.clone(),
            event_bus: self.event_bus.clone(),
            config: self.config.clone(),
        };

        let app = create_app(app_state);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| ObservabilityError::Dashboard(e.to_string()))?;

        println!(
            "🎯 LangGraph Observability Dashboard running on http://{}",
            addr
        );

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("Dashboard server error: {}", e);
            }
        });

        Ok(())
    }

    /// Shutdown the dashboard
    pub async fn shutdown(&self) -> ObservabilityResult<()> {
        if let Some(handle) = &self.server_handle {
            handle.abort();
        }
        Ok(())
    }
}

/// Application state for the dashboard
#[derive(Clone)]
struct AppState {
    storage: Arc<RwLock<dyn ObservabilityStorage>>,
    event_bus: EventBus,
    config: DashboardConfig,
}

/// Create the Axum application
fn create_app(state: AppState) -> Router {
    let mut app = Router::new()
        // API routes
        .route("/api/runs/list", get(list_runs))
        .route("/api/runs/:run_id", get(get_run))
        .route("/api/runs/:run_id/spans", get(get_run_spans))
        .route("/api/runs/:run_id/prompts", get(get_run_prompts))
        .route("/api/metrics/current", get(get_metrics))
        .route("/api/prompts/list", get(get_prompts))
        .route("/api/health", get(health_check))
        // WebSocket for real-time events
        .route("/ws", get(websocket_handler))
        // Dioxus SSR routes for all frontend paths
        .route("/", get(serve_dioxus_app))
        .route("/runs", get(serve_dioxus_app))
        .route("/runs/:run_id", get(serve_dioxus_app))
        .route("/metrics", get(serve_dioxus_app))
        .route("/prompts", get(serve_dioxus_app))
        // Static assets for Dioxus
        .route("/pkg/*file", get(serve_static_assets))
        .with_state(state);

    // Add CORS layer
    app.layer(CorsLayer::permissive())
}

/// List runs endpoint
async fn list_runs(
    Query(params): Query<RunListParams>,
    State(state): State<AppState>,
) -> Result<Json<RunListResponse>, StatusCode> {
    let filters = RunFilters {
        graph_id: params.graph_id,
        status: params.status,
        start_time_range: None, // TODO: Parse from query params
        metadata: HashMap::new(),
    };

    let storage = state.storage.read().await;
    match storage
        .list_runs(&filters, params.limit, params.offset)
        .await
    {
        Ok(runs) => Ok(Json(RunListResponse { runs })),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get specific run endpoint
async fn get_run(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let storage = state.storage.read().await;
    match storage.get_run(&run_id).await {
        Ok(Some(run)) => Ok(Json(serde_json::to_value(run).unwrap())),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get run spans endpoint
async fn get_run_spans(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let storage = state.storage.read().await;
    match storage.get_spans(&run_id).await {
        Ok(spans) => Ok(Json(serde_json::to_value(spans).unwrap())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get run prompts endpoint
async fn get_run_prompts(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let storage = state.storage.read().await;
    match storage.get_prompts(&run_id).await {
        Ok(prompts) => Ok(Json(serde_json::to_value(prompts).unwrap())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get metrics endpoint
async fn get_metrics(
    Query(params): Query<MetricsParams>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let storage = state.storage.read().await;
    let start_time = chrono::Utc::now() - chrono::Duration::hours(params.hours.unwrap_or(24));
    let end_time = chrono::Utc::now();

    match storage.get_metrics(start_time, end_time).await {
        Ok(metrics) => Ok(Json(serde_json::to_value(metrics).unwrap())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Health check endpoint
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now()
    }))
}

/// WebSocket handler for real-time events
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_websocket(socket, state))
}

/// Handle WebSocket connections
async fn handle_websocket(socket: axum::extract::ws::WebSocket, state: AppState) {
    use axum::extract::ws::Message;
    use futures::{sink::SinkExt, stream::StreamExt};

    let (mut sender, mut receiver) = socket.split();
    let mut event_stream = EventStream::new(state.event_bus.receiver());

    // Send initial connection message
    let _ = sender
        .send(Message::Text(
            serde_json::json!({
                "type": "connected",
                "timestamp": chrono::Utc::now()
            })
            .to_string(),
        ))
        .await;

    // Handle incoming messages and outgoing events
    tokio::select! {
        // Handle incoming WebSocket messages
        _ = async {
            while let Some(msg) = receiver.next().await {
                if let Ok(Message::Text(text)) = msg {
                    // Handle client messages (filters, subscriptions, etc.)
                    if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                        match client_msg {
                            ClientMessage::Subscribe { event_types } => {
                                // Update event stream filters
                                println!("Client subscribed to: {:?}", event_types);
                            }
                            ClientMessage::Unsubscribe => {
                                // Clear filters
                                println!("Client unsubscribed");
                            }
                        }
                    }
                }
            }
        } => {},

        // Send events to client
        _ = async {
            while let Ok(event) = event_stream.next().await {
                let event_json = serde_json::to_string(&event).unwrap_or_default();
                if sender.send(Message::Text(event_json)).await.is_err() {
                    break;
                }
            }
        } => {}
    }
}

/// Serve the Dioxus app with SSR
async fn serve_dioxus_app() -> Html<String> {
    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>LangGraph Observability Dashboard</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 0; padding: 20px; background-color: #f5f5f5; }}
        .header {{ background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; padding: 20px; border-radius: 8px; margin-bottom: 20px; }}
        .nav {{ background: white; padding: 15px; border-radius: 8px; margin-bottom: 20px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .nav a {{ margin-right: 20px; text-decoration: none; color: #333; font-weight: bold; }}
        .nav a:hover {{ color: #667eea; }}
        .card {{ background: white; padding: 20px; border-radius: 8px; margin-bottom: 20px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .metrics-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 20px; }}
        .metric-card {{ background: white; padding: 15px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); text-align: center; }}
        .metric-value {{ font-size: 2em; font-weight: bold; color: #667eea; }}
        .runs-list {{ display: flex; flex-direction: column; gap: 10px; }}
        .run-item {{ background: white; padding: 15px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .run-header {{ display: flex; justify-content: space-between; align-items: center; }}
        .run-title {{ font-weight: bold; color: #667eea; text-decoration: none; }}
        .run-status {{ padding: 4px 8px; border-radius: 4px; font-size: 0.8em; font-weight: bold; }}
        .run-status.completed {{ background: #d4edda; color: #155724; }}
        .run-status.failed {{ background: #f8d7da; color: #721c24; }}
        .run-status.running {{ background: #fff3cd; color: #856404; }}
        .loading {{ text-align: center; padding: 20px; color: #666; }}
        .dashboard-content {{ display: flex; flex-direction: column; gap: 20px; }}
        .timeline {{ position: relative; padding-left: 20px; }}
        .timeline-item {{ margin: 10px 0; padding: 15px; background: white; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .prompt-item {{ background: white; padding: 15px; border-radius: 8px; margin: 10px 0; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .prompt-content {{ background: #f8f9fa; padding: 10px; border-radius: 4px; white-space: pre-wrap; margin: 10px 0; }}
    </style>
</head>
<body>
    <div id="main"></div>
    <script type="module">
        import init, {{ main }} from '/pkg/langgraph_observability.js';
        
        async function run() {{
            await init();
            main();
        }}
        
        run();
    </script>
</body>
</html>"#
    );
    Html(html)
}

/// Serve static assets (WASM files, etc.)
async fn serve_static_assets(Path(_file): Path<String>) -> Result<&'static str, StatusCode> {
    // For now, return a 404 - in a real implementation, you'd serve the actual WASM files
    Err(StatusCode::NOT_FOUND)
}

/// List all prompts endpoint
async fn get_prompts(
    State(_state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    // For now, return empty list - this would be implemented with actual storage
    Ok(Json(vec![]))
}

/// Query parameters for listing runs
#[derive(Debug, Deserialize)]
struct RunListParams {
    graph_id: Option<String>,
    status: Option<crate::storage::RunStatus>,
    limit: Option<u32>,
    offset: Option<u32>,
}

/// Response for listing runs
#[derive(Debug, Serialize)]
struct RunListResponse {
    runs: Vec<crate::storage::GraphRun<serde_json::Value>>,
}

/// Query parameters for metrics
#[derive(Debug, Deserialize)]
struct MetricsParams {
    hours: Option<i64>,
}

/// Client messages over WebSocket
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    Subscribe { event_types: Vec<String> },
    Unsubscribe,
}

/// Create a minimal HTML template for the dashboard
const DASHBOARD_HTML: &str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>LangGraph Observability Dashboard</title>
    <style>
        body {
            font-family: Arial, sans-serif;
            margin: 0;
            padding: 20px;
            background-color: #f5f5f5;
        }
        .header {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 20px;
        }
        .nav {
            background: white;
            padding: 15px;
            border-radius: 8px;
            margin-bottom: 20px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }
        .nav a {
            margin-right: 20px;
            text-decoration: none;
            color: #333;
            font-weight: bold;
        }
        .nav a:hover {
            color: #667eea;
        }
        .card {
            background: white;
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 20px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }
        .metrics-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 20px;
        }
        .metric-card {
            background: white;
            padding: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
            text-align: center;
        }
        .metric-value {
            font-size: 2em;
            font-weight: bold;
            color: #667eea;
        }
        .metric-label {
            color: #666;
            margin-top: 5px;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🔍 LangGraph Observability Dashboard</h1>
        <p>Monitor and debug your LangGraph applications</p>
    </div>

    <div class="nav">
        <a href="/">Dashboard</a>
        <a href="/runs">Runs</a>
        <a href="/metrics">Metrics</a>
        <a href="/prompts">Prompts</a>
    </div>

    <div class="metrics-grid">
        <div class="metric-card">
            <div class="metric-value" id="total-runs">-</div>
            <div class="metric-label">Total Runs</div>
        </div>
        <div class="metric-card">
            <div class="metric-value" id="active-runs">-</div>
            <div class="metric-label">Active Runs</div>
        </div>
        <div class="metric-card">
            <div class="metric-value" id="avg-duration">-</div>
            <div class="metric-label">Avg Duration (s)</div>
        </div>
        <div class="metric-card">
            <div class="metric-value" id="success-rate">-</div>
            <div class="metric-label">Success Rate</div>
        </div>
    </div>

    <div class="card">
        <h2>Recent Runs</h2>
        <div id="recent-runs">Loading...</div>
    </div>

    <div class="card">
        <h2>Real-time Events</h2>
        <div id="events" style="height: 300px; overflow-y: auto; background: #f8f9fa; padding: 10px; border-radius: 4px;">
            <div style="color: #666;">Connecting to event stream...</div>
        </div>
    </div>

    <script>
        // Initialize WebSocket connection for real-time events
        const ws = new WebSocket('ws://localhost:3000/ws');
        const eventsDiv = document.getElementById('events');

        ws.onopen = function(event) {
            eventsDiv.innerHTML = '<div style="color: green;">Connected to event stream</div>';
        };

        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            const eventDiv = document.createElement('div');
            eventDiv.style.marginBottom = '5px';
            eventDiv.style.padding = '5px';
            eventDiv.style.backgroundColor = 'white';
            eventDiv.style.borderRadius = '3px';
            eventDiv.innerHTML = `<strong>${data.type || data.event_type || 'event'}:</strong> ${JSON.stringify(data)}`;
            eventsDiv.insertBefore(eventDiv, eventsDiv.firstChild);

            // Keep only last 50 events
            while (eventsDiv.children.length > 50) {
                eventsDiv.removeChild(eventsDiv.lastChild);
            }
        };

        ws.onclose = function(event) {
            eventsDiv.innerHTML = '<div style="color: red;">Connection closed</div>';
        };

        ws.onerror = function(error) {
            eventsDiv.innerHTML = '<div style="color: red;">Connection error</div>';
        };

        // Load dashboard data
        async function loadDashboard() {
            try {
                // Load runs
                const runsResponse = await fetch('/api/runs?limit=10');
                const runsData = await runsResponse.json();
                
                document.getElementById('total-runs').textContent = runsData.runs.length;
                
                const activeRuns = runsData.runs.filter(r => r.status === 'Running').length;
                document.getElementById('active-runs').textContent = activeRuns;

                // Display recent runs
                const recentRunsDiv = document.getElementById('recent-runs');
                if (runsData.runs.length > 0) {
                    const runsList = runsData.runs.map(run => `
                        <div style="padding: 10px; margin: 5px 0; background: #f8f9fa; border-radius: 4px;">
                            <strong>Run ID:</strong> ${run.id}<br>
                            <strong>Graph:</strong> ${run.graph_id}<br>
                            <strong>Status:</strong> ${run.status}<br>
                            <strong>Started:</strong> ${new Date(run.start_time).toLocaleString()}
                        </div>
                    `).join('');
                    recentRunsDiv.innerHTML = runsList;
                } else {
                    recentRunsDiv.innerHTML = '<div style="color: #666;">No runs found</div>';
                }

            } catch (error) {
                console.error('Error loading dashboard:', error);
            }
        }

        // Load data on page load
        loadDashboard();

        // Refresh data every 30 seconds
        setInterval(loadDashboard, 30000);
    </script>
</body>
</html>
"#;
