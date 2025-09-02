use dioxus::prelude::*;
use dioxus_router::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Routable, Clone)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Home {},
    #[route("/runs")]
    Runs {},
    #[route("/runs/:id")]
    RunDetail { id: String },
    #[route("/metrics")]
    Metrics {},
    #[route("/prompts")]
    Prompts {},
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphRun {
    pub id: String,
    pub graph_id: String,
    pub status: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub error: Option<String>,
    pub initial_state: serde_json::Value,
    pub final_state: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceSpan {
    pub id: String,
    pub name: String,
    pub operation_name: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PromptExecution {
    pub id: String,
    pub run_id: String,
    pub model: String,
    pub prompt: String,
    pub response: String,
    pub timestamp: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MetricsData {
    pub total_runs: u64,
    pub successful_runs: u64,
    pub failed_runs: u64,
    pub avg_duration: f64,
    pub memory_usage: u64,
    pub cpu_usage: f64,
}

pub fn App() -> Element {
    rsx! {
        Router::<Route> {}
    }
}

#[component]
fn Home() -> Element {
    let mut metrics = use_signal(|| None::<MetricsData>);
    let mut runs = use_signal(|| Vec::<GraphRun>::new());
    let mut loading = use_signal(|| true);

    use_effect(move || {
        spawn(async move {
            if let Ok(response) = gloo_net::http::Request::get("/api/metrics/current")
                .send()
                .await
            {
                if let Ok(data) = response.json::<MetricsData>().await {
                    metrics.set(Some(data));
                }
            }

            if let Ok(response) = gloo_net::http::Request::get("/api/runs/list?limit=5")
                .send()
                .await
            {
                if let Ok(data) = response.json::<Vec<GraphRun>>().await {
                    runs.set(data);
                }
            }

            loading.set(false);
        });
    });

    rsx! {
        div { class: "dashboard-home",
            Header {}

            if *loading.read() {
                div { class: "loading", "Loading dashboard..." }
            } else {
                div { class: "dashboard-content",
                    // Metrics Overview
                    if let Some(metrics_data) = metrics.read().as_ref() {
                        MetricsOverview { metrics: metrics_data.clone() }
                    }

                    // Recent Runs
                    RecentRuns { runs: runs.read().clone() }
                }
            }
        }
    }
}

#[component]
fn Header() -> Element {
    rsx! {
        header { class: "header",
            h1 { "LangGraph Observability Dashboard" }
            nav { class: "nav",
                Link { to: Route::Home {}, "Dashboard" }
                Link { to: Route::Runs {}, "Runs" }
                Link { to: Route::Metrics {}, "Metrics" }
                Link { to: Route::Prompts {}, "Prompts" }
            }
        }
    }
}

#[component]
fn MetricsOverview(metrics: MetricsData) -> Element {
    rsx! {
        div { class: "metrics-overview",
            h2 { "System Overview" }
            div { class: "metrics-grid",
                div { class: "metric-card",
                    h3 { "Total Runs" }
                    div { class: "metric-value", "{metrics.total_runs}" }
                }
                div { class: "metric-card",
                    h3 { "Success Rate" }
                    div { class: "metric-value",
                        if metrics.total_runs > 0 {
                            "{(metrics.successful_runs as f64 / metrics.total_runs as f64 * 100.0):.1}%"
                        } else {
                            "0.0%"
                        }
                    }
                }
                div { class: "metric-card",
                    h3 { "Avg Duration" }
                    div { class: "metric-value", "{metrics.avg_duration:.1}ms" }
                }
                div { class: "metric-card",
                    h3 { "Memory Usage" }
                    div { class: "metric-value", "{metrics.memory_usage / 1024 / 1024}MB" }
                }
            }
        }
    }
}

#[component]
fn RecentRuns(runs: Vec<GraphRun>) -> Element {
    rsx! {
        div { class: "recent-runs",
            h2 { "Recent Runs" }
            if runs.is_empty() {
                p { "No recent runs found." }
            } else {
                div { class: "runs-list",
                    for run in runs {
                        div { class: "run-item",
                            div { class: "run-header",
                                Link {
                                    to: Route::RunDetail { id: run.id.clone() },
                                    class: "run-title",
                                    "{run.id}"
                                }
                                span { class: "run-status {run.status.to_lowercase()}", "{run.status}" }
                            }
                            div { class: "run-details",
                                span { "Graph: {run.graph_id}" }
                                span { "Started: {run.start_time}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Runs() -> Element {
    let mut runs = use_signal(|| Vec::<GraphRun>::new());
    let mut loading = use_signal(|| true);

    use_effect(move || {
        spawn(async move {
            if let Ok(response) = gloo_net::http::Request::get("/api/runs/list").send().await {
                if let Ok(data) = response.json::<Vec<GraphRun>>().await {
                    runs.set(data);
                }
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "runs-page",
            Header {}

            h2 { "All Runs" }

            if *loading.read() {
                div { class: "loading", "Loading runs..." }
            } else if runs.read().is_empty() {
                p { "No runs found." }
            } else {
                div { class: "runs-table",
                    for run in runs.read().iter() {
                        div { class: "run-row",
                            div { class: "run-cell",
                                Link {
                                    to: Route::RunDetail { id: run.id.clone() },
                                    "{run.id}"
                                }
                            }
                            div { class: "run-cell", "{run.graph_id}" }
                            div { class: "run-cell",
                                span { class: "status {run.status.to_lowercase()}", "{run.status}" }
                            }
                            div { class: "run-cell", "{run.start_time}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RunDetail(id: String) -> Element {
    let mut run = use_signal(|| None::<GraphRun>);
    let mut spans = use_signal(|| Vec::<TraceSpan>::new());
    let mut loading = use_signal(|| true);

    use_effect(move || {
        let id = id.clone();
        spawn(async move {
            if let Ok(response) = gloo_net::http::Request::get(&format!("/api/runs/{}", id))
                .send()
                .await
            {
                if let Ok(data) = response.json::<GraphRun>().await {
                    run.set(Some(data));
                }
            }

            if let Ok(response) = gloo_net::http::Request::get(&format!("/api/runs/{}/spans", id))
                .send()
                .await
            {
                if let Ok(data) = response.json::<Vec<TraceSpan>>().await {
                    spans.set(data);
                }
            }

            loading.set(false);
        });
    });

    rsx! {
        div { class: "run-detail-page",
            Header {}

            if *loading.read() {
                div { class: "loading", "Loading run details..." }
            } else if let Some(run_data) = run.read().as_ref() {
                div { class: "run-detail",
                    h2 { "Run: {run_data.id}" }

                    div { class: "run-overview",
                        h3 { "Overview" }
                        div { class: "detail-grid",
                            div { "Graph ID: {run_data.graph_id}" }
                            div { "Status: {run_data.status}" }
                            div { "Started: {run_data.start_time}" }
                            if let Some(end_time) = &run_data.end_time {
                                div { "Ended: {end_time}" }
                            }
                        }
                    }

                    div { class: "execution-timeline",
                        h3 { "Execution Timeline" }
                        div { class: "timeline",
                            for span in spans.read().iter() {
                                div { class: "timeline-item",
                                    h4 { "{span.name}" }
                                    div { "Operation: {span.operation_name}" }
                                    div { "Started: {span.start_time}" }
                                    if let Some(end_time) = &span.end_time {
                                        div { "Ended: {end_time}" }
                                    }
                                    div { "Status: {span.status}" }
                                }
                            }
                        }
                    }
                }
            } else {
                div { "Run not found" }
            }
        }
    }
}

#[component]
fn Metrics() -> Element {
    let mut metrics = use_signal(|| None::<MetricsData>);
    let mut loading = use_signal(|| true);

    use_effect(move || {
        spawn(async move {
            if let Ok(response) = gloo_net::http::Request::get("/api/metrics/current")
                .send()
                .await
            {
                if let Ok(data) = response.json::<MetricsData>().await {
                    metrics.set(Some(data));
                }
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "metrics-page",
            Header {}

            h2 { "Performance Metrics" }

            if *loading.read() {
                div { class: "loading", "Loading metrics..." }
            } else if let Some(metrics_data) = metrics.read().as_ref() {
                MetricsOverview { metrics: metrics_data.clone() }
            } else {
                div { "No metrics available" }
            }
        }
    }
}

#[component]
fn Prompts() -> Element {
    let mut prompts = use_signal(|| Vec::<PromptExecution>::new());
    let mut loading = use_signal(|| true);

    use_effect(move || {
        spawn(async move {
            if let Ok(response) = gloo_net::http::Request::get("/api/prompts/list")
                .send()
                .await
            {
                if let Ok(data) = response.json::<Vec<PromptExecution>>().await {
                    prompts.set(data);
                }
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "prompts-page",
            Header {}

            h2 { "Prompt Executions" }

            if *loading.read() {
                div { class: "loading", "Loading prompts..." }
            } else if prompts.read().is_empty() {
                p { "No prompt executions found." }
            } else {
                div { class: "prompts-list",
                    for prompt in prompts.read().iter() {
                        div { class: "prompt-item",
                            h3 { "Model: {prompt.model}" }
                            div { class: "prompt-meta",
                                span { "Tokens: {prompt.input_tokens} → {prompt.output_tokens}" }
                                span { "Duration: {prompt.duration_ms}ms" }
                                span { "Time: {prompt.timestamp}" }
                            }
                            div { class: "prompt-content",
                                h4 { "Prompt:" }
                                pre { "{prompt.prompt}" }
                                h4 { "Response:" }
                                pre { "{prompt.response}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
