//! Example: State model – sequencing nodes and inspecting metadata

use langgraph_core::{StateGraph, GraphResult, ExecutionContext, START, END};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State { a: i32, b: i32 }

async fn add_a(mut s: State, _ctx: ExecutionContext) -> GraphResult<State> { s.a += 1; Ok(s) }
async fn add_b(mut s: State, _ctx: ExecutionContext) -> GraphResult<State> { s.b += 2; Ok(s) }

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
