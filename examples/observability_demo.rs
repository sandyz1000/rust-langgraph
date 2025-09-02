//! Observability Example - Demonstrates LangGraph observability and debugging features
//! 
//! This example shows how to use the LangGraph observability toolkit to monitor
//! and debug graph execution, similar to LangSmith functionality.

use langgraph_observability::{
    Observability, ObservabilityConfig, StorageConfig, TracingConfig, MetricsConfig, 
    DashboardConfig, PromptAnalyzer, PromptAnalysisConfig
};
use rust_langgraph::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatState {
    messages: Vec<String>,
    user_input: String,
    step_count: u32,
    context: String,
}

// Simulated LLM call node with observability
async fn llm_node(
    state: ChatState, 
    context: ExecutionContext
) -> GraphResult<ChatState> {
    println!("🤖 LLM processing: {}", state.user_input);
    
    // Simulate LLM processing time
    sleep(Duration::from_millis(500)).await;

    let mut messages = state.messages;
    
    // Simulate different responses based on input
    let response = if state.user_input.to_lowercase().contains("hello") {
        "Hello! How can I help you today?"
    } else if state.user_input.to_lowercase().contains("help") {
        "I'm here to assist you. What do you need help with?"
    } else {
        "I understand. Let me think about that..."
    };

    messages.push(format!("User: {}", state.user_input));
    messages.push(format!("Assistant: {}", response));

    Ok(ChatState {
        messages,
        user_input: state.user_input,
        step_count: state.step_count + 1,
        context: format!("Processed: {}", response),
    })
}

// Analysis node with metrics collection
async fn analysis_node(
    state: ChatState, 
    _context: ExecutionContext
) -> GraphResult<ChatState> {
    println!("📊 Analyzing conversation...");
    
    // Simulate analysis processing
    sleep(Duration::from_millis(200)).await;

    let sentiment = if state.context.contains("help") {
        "positive"
    } else {
        "neutral"
    };

    let mut messages = state.messages;
    messages.push(format!("Analysis: Sentiment is {}, {} messages processed", 
                         sentiment, state.step_count));

    Ok(ChatState {
        messages,
        user_input: state.user_input,
        step_count: state.step_count + 1,
        context: format!("{} | Sentiment: {}", state.context, sentiment),
    })
}

// Conditional logic for graph flow
fn should_continue(state: &ChatState) -> GraphResult<String> {
    if state.step_count < 4 {
        Ok("analysis".to_string())
    } else {
        Ok(END.to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 LangGraph Observability Example");
    println!("==================================\n");

    // Configure observability with comprehensive settings
    let observability_config = ObservabilityConfig::builder()
        .with_tracing(true)
        .with_metrics(true)
        .with_dashboard(true)
        .with_storage(StorageConfig::InMemory)
        .build();

    // Initialize observability system
    println!("🚀 Initializing observability system...");
    let observability = Observability::new(observability_config).await?;

    // Start the dashboard server in the background
    println!("🎯 Starting observability dashboard at http://localhost:3000");
    observability.start_dashboard("127.0.0.1:3000").await?;

    // Create graph observer
    let graph_observer = observability.create_graph_observer();

    // Build the graph
    println!("📊 Building LangGraph with observability...");
    let mut graph = StateGraph::<ChatState>::new();

    // Add nodes
    graph.add_node("llm", llm_node)?;
    graph.add_node("analysis", analysis_node)?;

    // Add edges
    graph.add_edge(START, "llm")?;
    graph.add_conditional_edge(
        "llm",
        should_continue,
        vec!["analysis".to_string(), END.to_string()],
    )?;
    graph.add_edge("analysis", "llm")?;

    // Set entry point
    graph.set_entry_point("llm")?;

    // Add metadata for observability
    graph.set_metadata("name", "Observability Chat Agent")?;
    graph.set_metadata("version", "1.0.0")?;
    graph.set_metadata("description", "Chat agent with comprehensive observability")?;

    // Compile the graph
    println!("⚙️  Compiling graph...");
    let compiled_graph = graph.compile().await?;

    println!("✅ Graph compiled successfully!");
    println!("   Nodes: {:?}", compiled_graph.list_nodes());
    println!("   Edges: {:?}", compiled_graph.list_edges());

    // Test multiple scenarios for observability data
    let test_scenarios = vec![
        "Hello there!",
        "I need help with my project",
        "What's the weather like?",
        "Can you help me understand this?",
    ];

    println!("\n🧪 Running test scenarios...");
    
    for (i, user_input) in test_scenarios.iter().enumerate() {
        println!("\n--- Scenario {} ---", i + 1);
        println!("User: {}", user_input);

        // Start observing this run
        let run_id = graph_observer.start_run("observability-chat-agent".to_string()).await?;
        println!("📝 Started run: {}", run_id);

        // Create initial state
        let initial_state = ChatState {
            messages: vec![],
            user_input: user_input.to_string(),
            step_count: 0,
            context: String::new(),
        };

        // Create execution context with observability metadata
        let mut config = GraphConfig::new();
        config.set_config("run_id", run_id.clone())?;
        config.set_config("scenario", format!("test_{}", i + 1))?;

        // Execute the graph with observability
        let start_time = std::time::Instant::now();
        
        match compiled_graph.invoke(initial_state, config).await {
            Ok(final_state) => {
                let duration = start_time.elapsed();
                println!("✅ Execution completed in {:?}", duration);
                println!("📊 Final state: {} messages", final_state.messages.len());
                
                // Simulate prompt analysis
                let prompt_analyzer = PromptAnalyzer::new(PromptAnalysisConfig::default());
                
                // Create a mock prompt execution for analysis
                let mock_prompt = langgraph_observability::storage::PromptExecution {
                    id: uuid::Uuid::new_v4().to_string(),
                    run_id: run_id.clone(),
                    node_id: "llm".to_string(),
                    model: "gpt-3.5-turbo".to_string(),
                    input: user_input.to_string(),
                    output: final_state.context.clone(),
                    token_usage: langgraph_observability::storage::TokenUsage {
                        input_tokens: (user_input.len() / 4) as u32, // Rough estimate
                        output_tokens: (final_state.context.len() / 4) as u32,
                        total_tokens: ((user_input.len() + final_state.context.len()) / 4) as u32,
                    },
                    start_time: chrono::Utc::now() - chrono::Duration::milliseconds(duration.as_millis() as i64),
                    end_time: chrono::Utc::now(),
                    metadata: std::collections::HashMap::new(),
                };

                // Analyze the prompt
                let analysis = prompt_analyzer.analyze_prompt(&mock_prompt).await?;
                println!("🔍 Prompt Analysis:");
                println!("   Quality Score: {:.1}", analysis.quality_score);
                if let Some(efficiency) = analysis.token_efficiency {
                    println!("   Token Efficiency: {:.1}%", efficiency.efficiency_score);
                    println!("   Estimated Cost: ${:.4}", efficiency.cost_estimate);
                }
                if !analysis.suggestions.is_empty() {
                    println!("   Suggestions: {:?}", analysis.suggestions);
                }

                // Publish completion event
                observability.event_bus().publish(
                    langgraph_observability::ObservabilityEvent::RunComplete {
                        run_id: run_id.clone(),
                        duration_ms: duration.as_millis() as u64,
                    }
                ).await?;

            }
            Err(e) => {
                println!("❌ Execution failed: {}", e);
                
                // Publish failure event
                observability.event_bus().publish(
                    langgraph_observability::ObservabilityEvent::RunFailed {
                        run_id: run_id.clone(),
                        error: e.to_string(),
                    }
                ).await?;
            }
        }

        // Add a small delay between scenarios
        sleep(Duration::from_millis(500)).await;
    }

    println!("\n🎯 Observability Dashboard Information:");
    println!("======================================");
    println!("🌐 Dashboard URL: http://localhost:3000");
    println!("📊 Metrics endpoint: http://localhost:3000/api/metrics");
    println!("🔍 Runs endpoint: http://localhost:3000/api/runs");
    println!("⚡ Real-time events: ws://localhost:3000/ws");
    
    println!("\n📋 Features Available:");
    println!("• Real-time graph execution monitoring");
    println!("• Detailed run traces and spans");
    println!("• Prompt analysis and optimization suggestions");
    println!("• Performance metrics and cost tracking");
    println!("• WebSocket-based live event streaming");
    println!("• Interactive debugging dashboard");

    println!("\n⏰ Keeping server running for 5 minutes...");
    println!("   Open http://localhost:3000 in your browser to explore!");
    
    // Keep the server running for demonstration
    sleep(Duration::from_secs(300)).await;

    // Cleanup
    println!("\n🧹 Shutting down observability system...");
    observability.shutdown().await?;
    
    println!("✅ Example completed successfully!");
    Ok(())
}
