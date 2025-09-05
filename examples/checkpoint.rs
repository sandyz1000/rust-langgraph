use anyhow::Result;
use chrono::Utc;
use langgraph_checkpoint::{
    Checkpoint, CheckpointConfig, CheckpointMetadata, Checkpointer, InMemoryCheckpointer,
};
use langgraph_core::{GraphConfig, GraphResult, StateGraph, StreamMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CounterState {
    value: i32,
}

// A tiny graph that increments twice; with checkpointing enabled, the engine will emit checkpoint events
async fn build_graph() -> GraphResult<langgraph_core::graph::CompiledGraph<CounterState>> {
    let mut graph = StateGraph::<CounterState>::new();

    graph.add_node("inc1", |mut s: CounterState, _ctx| async move {
        s.value += 1;
        Ok(s)
    })?;

    graph.add_node("inc2", |mut s: CounterState, _ctx| async move {
        s.value += 1;
        Ok(s)
    })?;

    graph.add_edge(langgraph_core::constants::START, "inc1")?;
    graph.add_edge("inc1", "inc2")?;
    graph.add_edge("inc2", langgraph_core::constants::END)?;

    graph.compile().await
}

#[tokio::main]
async fn main() -> Result<()> {
    // Part 1: Show engine-emitted checkpoint events while streaming
    let graph = build_graph().await?;

    let input = CounterState { value: 0 };
    let thread_id = format!("thread-{}", Utc::now().timestamp());

    let config = GraphConfig::new()
        .with_stream_mode(StreamMode::Checkpoints)
        .with_checkpointing(true)
        .with_thread_id(thread_id.clone());

    println!("-- Running graph with checkpointing enabled --");
    // Note: current engine returns the stream after execution; use state history to inspect checkpoints
    let _stream = graph.stream_with_config(input.clone(), config).await?;
    let snapshots = graph.get_state_history(&thread_id, None).await?;
    println!("Created {} state snapshots (checkpoints)", snapshots.len());
    for snap in snapshots {
        println!(
            " - snapshot id={} step={} value={}",
            snap.id, snap.step, snap.state.value
        );
    }

    // Part 2: Demonstrate using the InMemoryCheckpointer directly

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct SimpleState {
        count: i32,
    }

    let checkpointer = InMemoryCheckpointer::new();
    let cp_config = CheckpointConfig {
        thread_id: "demo-thread".into(),
        ..Default::default()
    };

    // Put a couple of checkpoints
    for i in 0..2 {
        let state = SimpleState { count: i };
        let checkpoint = Checkpoint::new(format!("cp-{}", i), state, i as u32);
        let metadata = CheckpointMetadata::new().with_thread_id(&cp_config.thread_id);
        Checkpointer::<SimpleState>::put(&checkpointer, &cp_config, checkpoint, metadata).await?;
        // tiny delay to create different timestamps ordering
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Get latest
    if let Some(tuple) = Checkpointer::<SimpleState>::get_latest(&checkpointer, &cp_config).await? {
        println!(
            "Latest checkpoint: id={} step={} count={}",
            tuple.checkpoint.id, tuple.checkpoint.step, tuple.checkpoint.state.count
        );
    }

    // List
    let list = Checkpointer::<SimpleState>::list(&checkpointer, &cp_config, None, None).await?;
    println!("All checkpoints (newest first):");
    for t in &list {
        println!(
            " - {} @ step {} (count={})",
            t.checkpoint.id, t.checkpoint.step, t.checkpoint.state.count
        );
    }

    // Get specific
    if let Some(t) = Checkpointer::<SimpleState>::get(&checkpointer, &cp_config, "cp-0").await? {
        println!(
            "Get cp-0 -> step={} count={}",
            t.checkpoint.step, t.checkpoint.state.count
        );
    }

    // Delete
    let deleted = Checkpointer::<SimpleState>::delete(&checkpointer, &cp_config, "cp-0").await?;
    println!("Deleted cp-0? {}", deleted);

    // Stats
    let stats = Checkpointer::<SimpleState>::stats(&checkpointer).await?;
    println!(
        "Stats: total={} threads={} avg_size={}B",
        stats.total_checkpoints, stats.unique_threads, stats.avg_checkpoint_size_bytes
    );

    Ok(())
}
