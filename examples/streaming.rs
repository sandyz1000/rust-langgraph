//! Streaming example demonstrating real-time event processing

use rust_langgraph::prelude::*;
use langgraph_core::{StreamEventData, StreamMode};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration};
use futures::StreamExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StreamState {
    input: String,
    tokens: Vec<String>,
    processing_stage: String,
    total_tokens: u32,
}

// Tokenizer node that simulates streaming token generation
async fn tokenize_input(
    state: StreamState,
    _context: ExecutionContext,
) -> GraphResult<StreamState> {
    println!("🔤 Tokenizing input: {}", state.input);
    
    let words: Vec<&str> = state.input.split_whitespace().collect();
    let mut tokens = Vec::new();
    
    // Simulate streaming by yielding tokens one by one
    for (i, word) in words.iter().enumerate() {
        // Add some processing delay to simulate real streaming
        sleep(Duration::from_millis(100)).await;
        
        tokens.push(word.to_string());
        
        // Send intermediate state update
        let _intermediate_state = StreamState {
            input: state.input.clone(),
            tokens: tokens.clone(),
            processing_stage: format!("Tokenizing... ({}/{})", i + 1, words.len()),
            total_tokens: tokens.len() as u32,
        };
        
        // In a real implementation, you would send this as a stream event
        println!("  📤 Token {}: '{}'", i + 1, word);
    }
    
    Ok(StreamState {
        input: state.input,
        tokens: tokens.clone(),
        processing_stage: "Tokenization complete".to_string(),
        total_tokens: tokens.len() as u32,
    })
}

// Analysis node that processes tokens
async fn analyze_tokens(
    state: StreamState,
    _context: ExecutionContext,
) -> GraphResult<StreamState> {
    println!("🔍 Analyzing {} tokens", state.tokens.len());
    
    // Simulate analysis processing
    for (i, token) in state.tokens.iter().enumerate() {
        sleep(Duration::from_millis(50)).await;
        println!("  🔎 Analyzing token {}: '{}'", i + 1, token);
    }
    
    Ok(StreamState {
        processing_stage: "Analysis complete".to_string(),
        ..state
    })
}

// Generation node that creates output
async fn generate_output(
    state: StreamState,
    _context: ExecutionContext,
) -> GraphResult<StreamState> {
    println!("✨ Generating output from {} tokens", state.tokens.len());
    
    // Simulate output generation
    let output_words = vec!["Generated", "response", "based", "on", "input"];
    
    for (i, word) in output_words.iter().enumerate() {
        sleep(Duration::from_millis(150)).await;
        println!("  📝 Generated word {}: '{}'", i + 1, word);
    }
    
    Ok(StreamState {
        processing_stage: "Generation complete".to_string(),
        ..state
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Streaming Processing Example");
    
    // Create state graph
    let mut graph = StateGraph::<StreamState>::new();
    
    // Add nodes
    graph.add_node("tokenize", tokenize_input)?;
    graph.add_node("analyze", analyze_tokens)?;
    graph.add_node("generate", generate_output)?;
    
    // Add edges
    graph.add_edge(START, "tokenize")?;
    graph.add_edge("tokenize", "analyze")?;
    graph.add_edge("analyze", "generate")?;
    graph.add_edge("generate", END)?;
    
    // Set entry point
    graph.set_entry_point("tokenize")?;
    
    graph.set_metadata("name", "Streaming Processor")?;
    
    // Compile the graph
    let compiled_graph = graph.compile().await?;
    
    // Test with different stream modes
    let test_inputs = vec![
        "Hello world how are you today",
        "This is a longer sentence with more words to process",
        "Short text",
    ];
    
    for (i, input) in test_inputs.iter().enumerate() {
        println!("\n{}", "=".repeat(60));
        println!("📋 Test {} - Input: '{}'", i + 1, input);
        println!("{}", "=".repeat(60));
        
        let initial_state = StreamState {
            input: input.to_string(),
            tokens: Vec::new(),
            processing_stage: "Starting".to_string(),
            total_tokens: 0,
        };
        
        // Demonstrate different streaming modes
        println!("\n🌊 Streaming with VALUES mode:");
        let config = GraphConfig {
            stream_mode: StreamMode::Values,
            ..Default::default()
        };
        let mut stream = compiled_graph.stream_with_config(initial_state.clone(), config).await?;
        
        while let Some(event) = stream.next().await {
            match event.event_type {
                StreamEventType::StateUpdate => {
                    if let StreamEventData::State(state) = &event.data {
                        println!("  📊 State: {} - {} tokens", 
                               state.processing_stage, state.total_tokens);
                    }
                }
                StreamEventType::NodeStart => {
                    if let Some(node) = &event.node {
                        println!("  🟡 Starting: {}", node);
                    }
                }
                StreamEventType::NodeComplete => {
                    if let Some(node) = &event.node {
                        println!("  ✅ Completed: {}", node);
                    }
                }
                StreamEventType::GraphComplete => {
                    println!("  🎉 Processing completed");
                    break;
                }
                _ => {}
            }
        }
        
        // Show final result with invoke
        println!("\n📤 Final result:");
        let final_state = compiled_graph.invoke(initial_state).await?;
        println!("  Tokens: {:?}", final_state.tokens);
        println!("  Total tokens: {}", final_state.total_tokens);
        println!("  Final stage: {}", final_state.processing_stage);
    }
    
    // Demonstrate updates mode
    println!("\n{}", "=".repeat(60));
    println!("📋 Testing UPDATES streaming mode");
    println!("{}", "=".repeat(60));
    
    let initial_state = StreamState {
        input: "Testing updates mode".to_string(),
        tokens: Vec::new(),
        processing_stage: "Starting".to_string(),
        total_tokens: 0,
    };
    
    let config = GraphConfig {
        stream_mode: StreamMode::Updates,
        ..Default::default()
    };
    let mut stream = compiled_graph.stream_with_config(initial_state, config).await?;
    
    while let Some(event) = stream.next().await {
        match event.event_type {
            StreamEventType::NodeComplete => {
                if let (Some(node), StreamEventData::State(state)) = (&event.node, &event.data) {
                    println!("  🔄 Update from {}: {} tokens processed", 
                           node, state.total_tokens);
                }
            }
            StreamEventType::GraphComplete => {
                println!("  🎯 All updates received");
                break;
            }
            _ => {}
        }
    }
    
    println!("\n🎉 Streaming example completed successfully!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_streaming_tokenization() {
        let mut graph = StateGraph::<StreamState>::new();
        graph.add_node("tokenize", tokenize_input).unwrap();
        graph.add_edge(START, "tokenize").unwrap();
        graph.add_edge("tokenize", END).unwrap();
        
        let compiled = graph.compile().await.unwrap();
        
        let initial_state = StreamState {
            input: "hello world".to_string(),
            tokens: Vec::new(),
            processing_stage: "Starting".to_string(),
            total_tokens: 0,
        };
        
        let result = compiled.invoke(initial_state).await.unwrap();
        assert_eq!(result.tokens, vec!["hello", "world"]);
        assert_eq!(result.total_tokens, 2);
    }
    
    #[tokio::test]
    async fn test_streaming_events() {
        let mut graph = StateGraph::<StreamState>::new();
        graph.add_node("tokenize", tokenize_input).unwrap();
        graph.add_node("analyze", analyze_tokens).unwrap();
        
        graph.add_edge(START, "tokenize").unwrap();
        graph.add_edge("tokenize", "analyze").unwrap();
        graph.add_edge("analyze", END).unwrap();
        
        let compiled = graph.compile().await.unwrap();
        
        let initial_state = StreamState {
            input: "test".to_string(),
            tokens: Vec::new(),
            processing_stage: "Starting".to_string(),
            total_tokens: 0,
        };
        
        let mut stream = compiled.stream(initial_state).await.unwrap();
        let mut events = Vec::new();
        
        while let Some(event) = stream.next().await {
            events.push(event.event_type);
            if matches!(event.event_type, StreamEventType::GraphComplete) {
                break;
            }
        }
        
        // Should have start, complete events for each node plus graph complete
        assert!(events.len() >= 3);
        assert!(events.contains(&StreamEventType::GraphComplete));
    }
}
