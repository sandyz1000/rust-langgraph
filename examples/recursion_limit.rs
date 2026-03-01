//! Example: Recursion limit safeguard

use langgraph_core::{
    ExecutionContext, GraphResult, LangGraphError, StateGraph, StateUpdate, START,
};
use serde::{Serialize, Deserialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoopState { n: u32 }

async fn loop_node(mut s: LoopState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    s.n += 1; // just keep incrementing; graph edges will loop
    let mut update = StateUpdate::new();
    update.insert("n".to_string(), Value::from(s.n));
    Ok(update)
}

#[tokio::main]
async fn main() -> GraphResult<()> {
    let mut graph = StateGraph::<LoopState>::new();
    graph.add_node("loop", loop_node)?;
    graph.add_edge(START, "loop")?;
    // loop back to itself (END is never reached)
    graph.add_edge("loop", "loop")?;

    let app = graph.compile().await?;
    match app.invoke(LoopState { n: 0 }).await {
        Ok(final_state) => println!("Finished unexpectedly: {:?}", final_state),
        Err(LangGraphError::RecursionLimit { .. }) => println!("Recursion limit hit as expected"),
        Err(e) => println!("Other error: {e}"),
    }
    Ok(())
}
