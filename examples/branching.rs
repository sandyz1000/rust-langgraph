//! Example: Branching based on state (similar to Python branching)

use langgraph_core::{ExecutionContext, GraphResult, StateGraph, StateUpdate, END, START};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Counter {
    value: i32,
}

async fn increment(mut s: Counter, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    s.value += 1;
    let mut update = StateUpdate::new();
    update.insert("value".to_string(), Value::from(s.value));
    Ok(update)
}

async fn even(mut s: Counter, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    // do something for even numbers
    s.value += 10;
    let mut update = StateUpdate::new();
    update.insert("value".to_string(), Value::from(s.value));
    Ok(update)
}

async fn odd(mut s: Counter, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    // do something for odd numbers
    s.value += 100;
    let mut update = StateUpdate::new();
    update.insert("value".to_string(), Value::from(s.value));
    Ok(update)
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
