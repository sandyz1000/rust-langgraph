//! Basic agent example demonstrating core LangGraph functionality

use rust_langgraph::prelude::*;
use langgraph_core::{StateUpdate, StreamEventData};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use futures::StreamExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentState {
    messages: Vec<String>,
    user_input: String,
    step_count: u32,
}

// Simple chatbot node that echoes user input
async fn chatbot_node(state: AgentState, _context: ExecutionContext) -> GraphResult<StateUpdate> {
    println!("Chatbot processing: {}", state.user_input);

    let mut messages = state.messages;
    messages.push(format!("User: {}", state.user_input));
    messages.push(format!(
        "Bot: I received your message: '{}'",
        state.user_input
    ));

    let mut update = StateUpdate::new();
    update.insert("messages".to_string(), serde_json::to_value(messages)?);
    update.insert("user_input".to_string(), Value::from(state.user_input));
    update.insert("step_count".to_string(), Value::from(state.step_count + 1));
    Ok(update)
}

// Analysis node that provides feedback
async fn analysis_node(state: AgentState, _context: ExecutionContext) -> GraphResult<StateUpdate> {
    println!("Analysis node processing {} messages", state.messages.len());

    let mut messages = state.messages;
    messages.push(format!(
        "Analysis: Processed {} conversation turns",
        state.step_count
    ));

    let mut update = StateUpdate::new();
    update.insert("messages".to_string(), serde_json::to_value(messages)?);
    update.insert("user_input".to_string(), Value::from(state.user_input));
    update.insert("step_count".to_string(), Value::from(state.step_count + 1));
    Ok(update)
}

// Conditional function to decide next node
fn should_continue(state: &AgentState) -> GraphResult<String> {
    if state.step_count < 3 {
        Ok("analysis".to_string())
    } else {
        Ok(END.to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Basic LangGraph Agent Example");

    // Create a new state graph
    let mut graph = StateGraph::<AgentState>::new();

    // Add nodes to the graph
    graph.add_node("chatbot", chatbot_node)?;
    graph.add_node("analysis", analysis_node)?;

    // Add edges
    graph.add_edge(START, "chatbot")?;
    graph.add_conditional_edge(
        "chatbot",
        should_continue,
        vec!["analysis".to_string(), END.to_string()],
    )?;
    graph.add_edge("analysis", END)?;

    // Set entry point
    graph.set_entry_point("chatbot")?;

    // Set metadata
    graph.set_metadata("name", "Basic Agent")?;
    graph.set_metadata("version", "1.0.0")?;

    // Compile the graph
    println!("\n� Compiling graph...");
    let compiled_graph = graph.compile().await?;

    println!("�📊 Graph structure:");
    println!("  Nodes: {:?}", compiled_graph.list_nodes());
    println!("  Edges: {:?}", compiled_graph.list_edges());

    // Create initial state
    let initial_state = AgentState {
        messages: vec![],
        user_input: "Hello, how are you?".to_string(),
        step_count: 0,
    };

    println!("\n💬 Initial state: {:?}", initial_state);

    // Execute the graph
    println!("\n🏃 Executing graph...");
    let final_state = compiled_graph.invoke(initial_state).await?;

    println!("\n✅ Final state:");
    println!("  Step count: {}", final_state.step_count);
    println!("  Messages:");
    for (i, message) in final_state.messages.iter().enumerate() {
        println!("    {}: {}", i + 1, message);
    }

    // Demonstrate streaming
    println!("\n🌊 Streaming execution:");
    let stream_state = AgentState {
        messages: vec![],
        user_input: "Tell me a joke!".to_string(),
        step_count: 0,
    };

    let mut stream = compiled_graph.stream(stream_state).await?;
    while let Some(event) = stream.next().await {
        match event.event_type {
            StreamEventType::NodeComplete => {
                if let Some(node) = event.node {
                    println!("  ✓ Completed node: {}", node);
                }
            }
            StreamEventType::StateUpdate => {
                if let StreamEventData::State(state) = event.data {
                    println!("  📝 State updated - step: {}", state.step_count);
                }
            }
            StreamEventType::GraphComplete => {
                println!("  🎉 Graph execution completed");
                break;
            }
            _ => {}
        }
    }

    println!("\n🎯 Example completed successfully!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_agent() {
        let mut graph = StateGraph::<AgentState>::new();
        graph.add_node("chatbot", chatbot_node).unwrap();
        graph.add_node("analysis", analysis_node).unwrap();
        graph.add_edge(START, "chatbot").unwrap();
        graph
            .add_conditional_edge(
                "chatbot",
                should_continue,
                vec!["analysis".to_string(), END.to_string()],
            )
            .unwrap();
        graph.add_edge("analysis", "chatbot").unwrap();

        let compiled = graph.compile().await.unwrap();

        let initial_state = AgentState {
            messages: vec![],
            // user_input: "Test message".to_string(),
            // step_count: 0,
            current_thought: todo!(),
            next_action: todo!(),
            tool_calls: todo!(),
            intermediate_steps: todo!(),
            is_final: todo!(),
        };

        let result = compiled.invoke(initial_state).await.unwrap();
        assert!(result.step_count > 0);
        assert!(!result.messages.is_empty());
    }
}
