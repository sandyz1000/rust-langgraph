//! Example: Using managed values in Rust LangGraph
//!
//! This example demonstrates:
//! - ConversationMemory for chat-style message retention
//! - ComputationCache for memoizing expensive results
//! - is_last_step and remaining_steps helpers mirroring Python managed values

use chrono::Utc;
use langgraph_core::managed::{is_last_step, remaining_steps};
use langgraph_core::managed::{ComputationCache, ConversationMemory, Message};
use langgraph_core::GraphResult;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> GraphResult<()> {
    // Conversation memory
    let memory = ConversationMemory::new();
    memory
        .add_message(Message {
            id: "1".into(),
            role: "user".into(),
            content: "Hello".into(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        })
        .await?;
    memory
        .add_message(Message {
            id: "2".into(),
            role: "assistant".into(),
            content: "Hi! How can I help?".into(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        })
        .await?;
    let msgs = memory.get_messages().await;
    println!("Conversation messages: {}", msgs.len());

    // Computation cache
    let cache: ComputationCache<String, usize> = ComputationCache::new();
    let key = "expensive".to_string();
    if let Some(v) = cache.get(&key).await {
        println!("Cache hit: {v}");
    } else {
        let value = 42usize; // pretend expensive
        cache.put(key.clone(), value).await?;
        println!("Cache miss -> stored {value}");
    }

    // Last-step helpers (you'd typically get ctx and stop from the runtime)
    let mut ctx = langgraph_core::ExecutionContext::new("node", "graph");
    ctx.step = 2; // zero-based
    let stop = 3; // total steps
    println!(
        "is_last_step={} remaining_steps={}",
        is_last_step(&ctx, stop),
        remaining_steps(&ctx, stop)
    );

    Ok(())
}
