//! Lightweight benchmark-style example against reference behaviors.
//! Run with: `cargo run --example reference_benchmark`

use std::time::Instant;

use rust_langgraph::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchState {
    value: i32,
}

async fn inc_node(state: BenchState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("value".to_string(), serde_json::json!(state.value + 1));
    Ok(update)
}

#[tokio::main]
async fn main() -> GraphResult<()> {
    let mut graph = StateGraph::<BenchState>::new();
    graph.add_node("inc", inc_node)?;
    graph.add_edge(START, "inc")?;
    graph.add_edge("inc", END)?;

    let app = graph.compile().await?;

    let iterations = 1_000usize;
    let started = Instant::now();

    for _ in 0..iterations {
        let _ = app.invoke(BenchState { value: 0 }).await?;
    }

    let elapsed = started.elapsed();
    let avg_micros = (elapsed.as_micros() as f64) / (iterations as f64);

    println!("Reference benchmark");
    println!("iterations: {}", iterations);
    println!("total: {:?}", elapsed);
    println!("avg_us_per_invoke: {:.2}", avg_micros);

    Ok(())
}
