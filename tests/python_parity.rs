use futures::StreamExt;
use langgraph_core::{StreamEventData, StreamMode};
use rust_langgraph::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ParentState {
    value: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ChildState {
    inner: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ChannelState {
    counter: i32,
    messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct NumericState {
    value: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct StringState {
    text: String,
}

async fn inc_node(state: ParentState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("value".to_string(), serde_json::json!(state.value + 1));
    Ok(update)
}

async fn child_node(state: ChildState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("inner".to_string(), serde_json::json!(state.inner + 10));
    Ok(update)
}

async fn ch_one(_state: ChannelState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("counter".to_string(), serde_json::json!(1));
    update.insert("messages".to_string(), serde_json::json!(["a"]));
    Ok(update)
}

async fn ch_two(_state: ChannelState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("counter".to_string(), serde_json::json!(2));
    update.insert("messages".to_string(), serde_json::json!(["b"]));
    Ok(update)
}

async fn num_one(_state: NumericState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("value".to_string(), serde_json::json!(3));
    Ok(update)
}

async fn num_two(_state: NumericState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("value".to_string(), serde_json::json!(9));
    Ok(update)
}

async fn str_one(_state: StringState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("text".to_string(), serde_json::json!("hello"));
    Ok(update)
}

async fn str_two(_state: StringState, _ctx: ExecutionContext) -> GraphResult<StateUpdate> {
    let mut update = StateUpdate::new();
    update.insert("text".to_string(), serde_json::json!(" world"));
    Ok(update)
}

#[tokio::test]
async fn parity_stream_tasks_mode() {
    let mut graph = StateGraph::<ParentState>::new();
    graph.add_node("inc", inc_node).unwrap();
    graph.add_edge(START, "inc").unwrap();
    graph.add_edge("inc", END).unwrap();

    let app = graph.compile().await.unwrap();
    let mut stream = app
        .stream_with_config(
            ParentState { value: 0 },
            GraphConfig::new().with_stream_mode(StreamMode::Tasks),
        )
        .await
        .unwrap();

    let mut saw_start = false;
    let mut saw_complete = false;
    let mut saw_state_update = false;

    while let Some(event) = stream.next().await {
        match event.event_type {
            StreamEventType::NodeStart => saw_start = true,
            StreamEventType::NodeComplete => saw_complete = true,
            StreamEventType::StateUpdate => saw_state_update = true,
            StreamEventType::GraphComplete => break,
            _ => {}
        }
    }

    assert!(saw_start);
    assert!(saw_complete);
    assert!(!saw_state_update);
}

#[tokio::test]
async fn parity_stream_updates_mode_payload() {
    let mut graph = StateGraph::<ParentState>::new();
    graph.add_node("inc", inc_node).unwrap();
    graph.add_edge(START, "inc").unwrap();
    graph.add_edge("inc", END).unwrap();

    let app = graph.compile().await.unwrap();
    let mut stream = app
        .stream_with_config(
            ParentState { value: 0 },
            GraphConfig::new().with_stream_mode(StreamMode::Updates),
        )
        .await
        .unwrap();

    let mut saw_update_payload = false;
    while let Some(event) = stream.next().await {
        if matches!(event.event_type, StreamEventType::StateUpdate)
            && matches!(event.data, StreamEventData::Update(_))
        {
            saw_update_payload = true;
        }
        if matches!(event.event_type, StreamEventType::GraphComplete) {
            break;
        }
    }

    assert!(saw_update_payload);
}

#[tokio::test]
async fn parity_subgraph_transform() {
    let mut child = StateGraph::<ChildState>::new();
    child.add_node("child_inc", child_node).unwrap();
    child.add_edge(START, "child_inc").unwrap();
    child.add_edge("child_inc", END).unwrap();
    let child_compiled = child.compile().await.unwrap();

    let mut parent = StateGraph::<ParentState>::new();
    parent
        .add_subgraph_with_transform(
            "child_flow",
            child_compiled,
            |state: &ParentState| Ok(ChildState { inner: state.value }),
            |_parent: &ParentState, sub: &ChildState| {
                let mut update = StateUpdate::new();
                update.insert("value".to_string(), serde_json::json!(sub.inner));
                Ok(update)
            },
        )
        .unwrap();
    parent.add_edge(START, "child_flow").unwrap();
    parent.add_edge("child_flow", END).unwrap();

    let app = parent.compile().await.unwrap();
    let out = app.invoke(ParentState { value: 2 }).await.unwrap();
    assert_eq!(out.value, 12);
}

#[tokio::test]
async fn parity_interrupt_resume() {
    let mut graph = StateGraph::<ParentState>::new();
    graph.add_node("inc", inc_node).unwrap();
    graph.add_edge(START, "inc").unwrap();
    graph.add_edge("inc", END).unwrap();

    let app = graph.compile().await.unwrap();
    let thread_id = uuid::Uuid::new_v4().to_string();

    let first = app
        .invoke_outcome_with_config(
            ParentState { value: 0 },
            GraphConfig::new()
                .with_thread_id(thread_id.clone())
                .with_interrupt_before("inc"),
        )
        .await
        .unwrap();

    assert!(matches!(first, InvokeOutcome::Interrupted(_)));

    let resumed = app.resume(&thread_id).await.unwrap();
    match resumed {
        InvokeOutcome::Completed(state) => assert_eq!(state.value, 1),
        InvokeOutcome::Interrupted(_) => panic!("resume should complete"),
    }
}

#[tokio::test]
async fn parity_channels_api_merge_behavior() {
    let mut graph = StateGraph::<ChannelState>::new();
    graph.add_node("one", ch_one).unwrap();
    graph.add_node("two", ch_two).unwrap();

    graph
        .set_channel_type(
            "counter",
            ChannelType::BinaryOp {
                reducer: BinaryOpReducer::Add,
            },
        )
        .unwrap();
    graph
        .set_channel_type("messages", ChannelType::Accumulator)
        .unwrap();

    graph.add_edge(START, "one").unwrap();
    graph.add_edge(START, "two").unwrap();
    graph.add_edge("one", END).unwrap();
    graph.add_edge("two", END).unwrap();

    let app = graph.compile().await.unwrap();
    let out = app
        .invoke(ChannelState {
            counter: 0,
            messages: vec![],
        })
        .await
        .unwrap();

    assert_eq!(out.counter, 3);
    assert_eq!(out.messages.len(), 2);
    assert!(out.messages.contains(&"a".to_string()));
    assert!(out.messages.contains(&"b".to_string()));
}

#[tokio::test]
async fn parity_channels_binaryop_max() {
    let mut graph = StateGraph::<NumericState>::new();
    graph.add_node("one", num_one).unwrap();
    graph.add_node("two", num_two).unwrap();
    graph
        .set_channel_type(
            "value",
            ChannelType::BinaryOp {
                reducer: BinaryOpReducer::Max,
            },
        )
        .unwrap();
    graph.add_edge(START, "one").unwrap();
    graph.add_edge(START, "two").unwrap();
    graph.add_edge("one", END).unwrap();
    graph.add_edge("two", END).unwrap();

    let app = graph.compile().await.unwrap();
    let out = app.invoke(NumericState { value: 0 }).await.unwrap();
    assert_eq!(out.value, 9);
}

#[tokio::test]
async fn parity_channels_binaryop_min() {
    let mut graph = StateGraph::<NumericState>::new();
    graph.add_node("one", num_one).unwrap();
    graph.add_node("two", num_two).unwrap();
    graph
        .set_channel_type(
            "value",
            ChannelType::BinaryOp {
                reducer: BinaryOpReducer::Min,
            },
        )
        .unwrap();
    graph.add_edge(START, "one").unwrap();
    graph.add_edge(START, "two").unwrap();
    graph.add_edge("one", END).unwrap();
    graph.add_edge("two", END).unwrap();

    let app = graph.compile().await.unwrap();
    let out = app.invoke(NumericState { value: 100 }).await.unwrap();
    assert_eq!(out.value, 3);
}

#[tokio::test]
async fn parity_channels_binaryop_concat() {
    let mut graph = StateGraph::<StringState>::new();
    graph.add_node("one", str_one).unwrap();
    graph.add_node("two", str_two).unwrap();
    graph
        .set_channel_type(
            "text",
            ChannelType::BinaryOp {
                reducer: BinaryOpReducer::Concat,
            },
        )
        .unwrap();
    graph.add_edge(START, "one").unwrap();
    graph.add_edge(START, "two").unwrap();
    graph.add_edge("one", END).unwrap();
    graph.add_edge("two", END).unwrap();

    let app = graph.compile().await.unwrap();
    let out = app
        .invoke(StringState {
            text: String::new(),
        })
        .await
        .unwrap();
    assert_eq!(out.text, "hello world");
}
