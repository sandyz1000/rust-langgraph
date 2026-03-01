//! Example: State model – sequencing nodes and inspecting metadata

use langgraph_core::{ExecutionContext, GraphResult, StateGraph, StateUpdate, END, START};
use serde::{Serialize, Deserialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State { a: i32, b: i32 }

async fn add_a(mut s: State, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    s.a += 1;
    let mut update = StateUpdate::new();
    update.insert("a".to_string(), Value::from(s.a));
    Ok(update)
}

async fn add_b(mut s: State, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    s.b += 2;
    let mut update = StateUpdate::new();
    update.insert("b".to_string(), Value::from(s.b));
    Ok(update)
}

#[tokio::main]
async fn main() -> GraphResult<()> {
    let mut graph = StateGraph::<State>::new();
    graph.add_node("add_a", add_a)?;
    graph.add_node("add_b", add_b)?;
    graph.set_metadata("owner", "state-model-example")?;

    graph.add_edge(START, "add_a")?;
    graph.add_edge("add_a", "add_b")?;
    graph.add_edge("add_b", END)?;

    let app = graph.compile().await?;
    let out = app.invoke(State { a: 0, b: 0 }).await?;
    println!("State: a={}, b={}", out.a, out.b);
    Ok(())
}
