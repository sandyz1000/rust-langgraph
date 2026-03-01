//! Example: Using GraphConfig and streaming updates

use futures::StreamExt;
use langgraph_core::{
    ExecutionContext, GraphConfig, GraphResult, StateGraph, StateUpdate, StreamEventType,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Data {
    x: i32,
}

async fn work(mut s: Data, mut ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    // Read from config (if present)
    let bump: Option<i32> = ctx.get_config("bump");
    if let Some(b) = bump {
        s.x += b;
    }
    // Set some metadata
    ctx.set_metadata("worker", "work").ok();
    let mut update = StateUpdate::new();
    update.insert("x".to_string(), Value::from(s.x));
    Ok(update)
}

#[tokio::main]
async fn main() -> GraphResult<()> {
    let mut graph = StateGraph::<Data>::new();
    graph.add_node("work", work)?;
    graph.add_edge(langgraph_core::START, "work")?;
    graph.add_edge("work", langgraph_core::END)?;
    let app = graph.compile().await?;

    let cfg = GraphConfig::new()
        .with_stream_mode(langgraph_core::types::StreamMode::Updates)
        .with_config("bump", 5)?
        .with_debug(true);

    let mut stream = app.stream_with_config(Data { x: 10 }, cfg).await?;
    while let Some(ev) = stream.next().await {
        match ev.event_type {
            StreamEventType::GraphStart => println!("Graph started"),
            StreamEventType::StateUpdate => println!("State update @ step {}", ev.step),
            StreamEventType::GraphComplete => println!("Done"),
            _ => {}
        }
    }
    Ok(())
}
