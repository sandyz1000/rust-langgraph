//! Example: Channels API style state behavior (Python parity slice)
//!
//! Demonstrates per-key channel semantics:
//! - `messages` uses `Accumulator` behavior (append)
//! - `counter` uses `BinaryOp(add)` behavior (aggregate)

use langgraph_core::channels::{BinaryOpReducer, ChannelType};
use langgraph_core::{ExecutionContext, GraphResult, StateGraph, StateUpdate, END, START};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppState {
    counter: i32,
    messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdvancedState {
    score: i32,
    text: String,
}

async fn add_one(_state: AppState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("counter".to_string(), Value::from(1));
    update.insert("messages".to_string(), serde_json::json!(["node_a"]));
    Ok(update)
}

async fn add_two(_state: AppState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("counter".to_string(), Value::from(2));
    update.insert("messages".to_string(), serde_json::json!(["node_b"]));
    Ok(update)
}

async fn score_low(_state: AdvancedState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("score".to_string(), Value::from(42));
    update.insert("text".to_string(), Value::from("hello"));
    Ok(update)
}

async fn score_high(_state: AdvancedState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("score".to_string(), Value::from(99));
    update.insert("text".to_string(), Value::from(" world"));
    Ok(update)
}

#[tokio::main]
async fn main() -> GraphResult<()> {
    // Scenario 1: add + accumulator
    let mut graph = StateGraph::<AppState>::new();

    graph.add_node("node_a", add_one)?;
    graph.add_node("node_b", add_two)?;

    // Python LangGraph parity-style channel configuration per key
    graph.set_channel_type("counter", ChannelType::BinaryOp {
        reducer: BinaryOpReducer::Add,
    })?;
    graph.set_channel_type("messages", ChannelType::Accumulator)?;

    // Run both nodes in the same superstep
    graph.add_edge(START, "node_a")?;
    graph.add_edge(START, "node_b")?;
    graph.add_edge("node_a", END)?;
    graph.add_edge("node_b", END)?;

    let app = graph.compile().await?;
    let result = app
        .invoke(AppState {
            counter: 0,
            messages: vec![],
        })
        .await?;

    println!("counter: {}", result.counter); // 3
    println!("messages: {:?}", result.messages); // ["node_a", "node_b"] (order not guaranteed)

    // Scenario 2: max + concat
    let mut advanced_graph = StateGraph::<AdvancedState>::new();
    advanced_graph.add_node("score_low", score_low)?;
    advanced_graph.add_node("score_high", score_high)?;

    advanced_graph.set_channel_type(
        "score",
        ChannelType::BinaryOp {
            reducer: BinaryOpReducer::Max,
        },
    )?;
    advanced_graph.set_channel_type(
        "text",
        ChannelType::BinaryOp {
            reducer: BinaryOpReducer::Concat,
        },
    )?;

    advanced_graph.add_edge(START, "score_low")?;
    advanced_graph.add_edge(START, "score_high")?;
    advanced_graph.add_edge("score_low", END)?;
    advanced_graph.add_edge("score_high", END)?;

    let advanced_app = advanced_graph.compile().await?;
    let advanced_result = advanced_app
        .invoke(AdvancedState {
            score: 0,
            text: String::new(),
        })
        .await?;

    println!("score (max): {}", advanced_result.score); // 99
    println!("text (concat): {}", advanced_result.text); // "hello world"

    Ok(())
}
