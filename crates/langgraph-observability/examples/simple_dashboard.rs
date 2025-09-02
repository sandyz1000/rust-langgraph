use axum::{response::Html, routing::get, Router};
use std::net::SocketAddr;
use tokio::net::TcpListener;

// Simple example to demonstrate the basic Axum + Dioxus SSR setup
#[cfg(feature = "frontend")]
use dioxus::prelude::*;

#[cfg(feature = "frontend")]
fn App() -> Element {
    rsx! {
        html {
            head {
                title { "LangGraph Observability Dashboard" }
                meta { charset: "utf-8" }
                meta { name: "viewport", content: "width=device-width, initial-scale=1" }
                style {
                    r#"
                    body { font-family: Arial, sans-serif; margin: 0; padding: 20px; }
                    .header { background: #333; color: white; padding: 20px; margin: -20px -20px 20px -20px; }
                    .nav { display: flex; gap: 20px; margin: 20px 0; }
                    .nav a { color: #007bff; text-decoration: none; }
                    .nav a:hover { text-decoration: underline; }
                    .content { margin: 20px 0; }
                    "#
                }
            }
            body {
                div { class: "header",
                    h1 { "🔍 LangGraph Observability Dashboard" }
                    p { "Monitor and debug your LangGraph applications" }
                }
                nav { class: "nav",
                    a { href: "/", "Home" }
                    a { href: "/runs", "Runs" }
                    a { href: "/metrics", "Metrics" }
                    a { href: "/prompts", "Prompts" }
                }
                div { class: "content",
                    h2 { "Welcome to LangGraph Observability" }
                    p { "This is a Rust-native observability dashboard built with:" }
                    ul {
                        li { "🦀 Rust backend with Axum" }
                        li { "⚛️ Dioxus frontend (SSR)" }
                        li { "📊 Real-time metrics and tracing" }
                        li { "🔍 Prompt debugging capabilities" }
                    }

                    div { style: "margin-top: 30px;",
                        h3 { "Features" }
                        div { style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 20px;",
                            div { style: "border: 1px solid #ddd; padding: 15px; border-radius: 5px;",
                                h4 { "📈 Real-time Monitoring" }
                                p { "Track graph execution performance, latency, and throughput metrics in real-time." }
                            }
                            div { style: "border: 1px solid #ddd; padding: 15px; border-radius: 5px;",
                                h4 { "🔍 Prompt Analysis" }
                                p { "Debug and analyze LLM prompts, responses, and token usage patterns." }
                            }
                            div { style: "border: 1px solid #ddd; padding: 15px; border-radius: 5px;",
                                h4 { "📊 Distributed Tracing" }
                                p { "Visualize request flows across your LangGraph application components." }
                            }
                            div { style: "border: 1px solid #ddd; padding: 15px; border-radius: 5px;",
                                h4 { "⚡ High Performance" }
                                p { "Built with Rust for maximum performance and minimal resource usage." }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "frontend")]
async fn serve_dioxus() -> Html<String> {
    let mut vdom = VirtualDom::new(App);
    let _ = vdom.rebuild_in_place();
    Html(dioxus_ssr::render(&vdom))
}

#[cfg(not(feature = "frontend"))]
async fn serve_dioxus() -> Html<String> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>LangGraph Observability</title>
        <style>
            body { font-family: Arial, sans-serif; text-align: center; padding: 50px; }
            .message { background: #f0f0f0; padding: 20px; border-radius: 5px; margin: 20px; }
        </style>
    </head>
    <body>
        <h1>🔍 LangGraph Observability Dashboard</h1>
        <div class="message">
            <p>Frontend feature not enabled. To use the Dioxus frontend, build with:</p>
            <code>cargo run --features frontend</code>
        </div>
    </body>
    </html>
    "#
        .to_string(),
    )
}

async fn api_status() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    println!("🚀 Starting LangGraph Observability Dashboard");

    let app = Router::new()
        .route("/", get(serve_dioxus))
        .route("/api/status", get(api_status));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("🌐 Dashboard available at: http://{}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
