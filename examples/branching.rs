//! Example: Branching based on state (similar to Python branching)

use langgraph_core::{ExecutionContext, GraphResult, StateGraph, END, START};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Counter {
    value: i32,
}

async fn increment(mut s: Counter, _ctx: ExecutionContext) -> GraphResult<Counter> {
    s.value += 1;
    Ok(s)
}

async fn even(mut s: Counter, _ctx: ExecutionContext) -> GraphResult<Counter> {
    // do something for even numbers
    s.value += 10;
    Ok(s)
}

async fn odd(mut s: Counter, _ctx: ExecutionContext) -> GraphResult<Counter> {
    // do something for odd numbers
    s.value += 100;
    Ok(s)
}

#[tokio::main]
async fn main() -> GraphResult<()> {
    let mut graph = StateGraph::<Counter>::new();
    graph.add_node("increment", increment)?;
    graph.add_node("even", even)?;
    graph.add_node("odd", odd)?;

    graph.add_edge(START, "increment")?;

    // Branch from `increment` to either `even` or `odd`
    graph.add_conditional_edge(
        "increment",
        |state: &Counter| {
            if state.value % 2 == 0 {
                Ok("even".to_string())
            } else {
                Ok("odd".to_string())
            }
        },
        vec!["even".to_string(), "odd".to_string()],
    )?;

    // Both paths go to END
    graph.add_edge("even", END)?;
    graph.add_edge("odd", END)?;

    let app = graph.compile().await?;
    let out = app.invoke(Counter { value: 1 }).await?;
    println!("Final state: {:?}", out);
    Ok(())
}
