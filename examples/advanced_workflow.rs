//! Advanced agent example with checkpointing and human-in-the-loop

use rust_langgraph::prelude::*;
use rust_langgraph::checkpoint::InMemoryCheckpointer;
use langgraph_core::{StreamEventData, StreamMode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use futures::StreamExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowState {
    user_request: String,
    analysis: Option<String>,
    approval_needed: bool,
    approved: bool,
    result: Option<String>,
    step_count: u32,
}

// Analysis node
async fn analyze_request(
    state: WorkflowState,
    _context: ExecutionContext,
) -> GraphResult<WorkflowState> {
    println!("🔍 Analyzing request: {}", state.user_request);
    
    let analysis = format!("Analysis of '{}': This request requires careful processing", state.user_request);
    let approval_needed = state.user_request.to_lowercase().contains("important") || 
                         state.user_request.to_lowercase().contains("urgent");
    
    Ok(WorkflowState {
        analysis: Some(analysis),
        approval_needed,
        step_count: state.step_count + 1,
        ..state
    })
}

// Human approval node (simulated)
async fn human_approval(
    state: WorkflowState,
    _context: ExecutionContext,
) -> GraphResult<WorkflowState> {
    println!("⏸️  Requesting human approval for: {}", state.user_request);
    
    // In a real scenario, this would pause execution and wait for human input
    // For demo purposes, we'll auto-approve non-urgent requests
    let approved = !state.user_request.to_lowercase().contains("dangerous");
    
    if approved {
        println!("✅ Request approved");
    } else {
        println!("❌ Request rejected");
    }
    
    Ok(WorkflowState {
        approved,
        step_count: state.step_count + 1,
        ..state
    })
}

// Processing node
async fn process_request(
    state: WorkflowState,
    _context: ExecutionContext,
) -> GraphResult<WorkflowState> {
    println!("⚙️  Processing approved request");
    
    let result = if state.approved {
        format!("Successfully processed: {}", state.user_request)
    } else {
        "Request was not approved and cannot be processed".to_string()
    };
    
    Ok(WorkflowState {
        result: Some(result),
        step_count: state.step_count + 1,
        ..state
    })
}

// Conditional routing functions
fn needs_approval(state: &WorkflowState) -> GraphResult<String> {
    if state.approval_needed {
        Ok("human_approval".to_string())
    } else {
        Ok("process".to_string())
    }
}

fn should_process(state: &WorkflowState) -> GraphResult<String> {
    if state.approved {
        Ok("process".to_string())
    } else {
        Ok(END.to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Advanced Agent with Checkpointing Example");
    
    // Create checkpointer
    let _checkpointer = InMemoryCheckpointer::new();
    
    // Create state graph
    let mut graph = StateGraph::<WorkflowState>::new();
    
    // Add nodes
    graph.add_node("analyze", analyze_request)?;
    graph.add_node("human_approval", human_approval)?;
    graph.add_node("process", process_request)?;
    
    // Add edges
    graph.add_edge(START, "analyze")?;
    graph.add_conditional_edge("analyze", needs_approval, vec!["human_approval".to_string(), "process".to_string()])?;
    graph.add_conditional_edge("human_approval", should_process, vec!["process".to_string(), END.to_string()])?;
    graph.add_edge("process", END)?;
    
    // Set entry point
    graph.set_entry_point("analyze")?;
    
    // Set up checkpointing
    graph.set_metadata("name", "Advanced Workflow Agent")?;
    graph.set_metadata("version", "2.0.0")?;
    
    // Compile with checkpointer
    let compiled_graph = graph.compile().await?;
    
    println!("📊 Graph structure:");
    println!("  Nodes: {:?}", compiled_graph.list_nodes());
    
    // Test scenarios
    let scenarios = vec![
        ("Process this document", "routine"),
        ("This is an important decision", "needs_approval"),
        ("Urgent: Handle this dangerous request", "rejected"),
    ];
    
    for (request, scenario_type) in scenarios {
        println!("\n{}", "=".repeat(60));
        println!("📋 Scenario: {} ({})", request, scenario_type);
        println!("{}", "=".repeat(60));
        
        let thread_id = Uuid::new_v4().to_string();
        let config = GraphConfig {
            stream_mode: StreamMode::Values,
            thread_id: Some(thread_id.clone()),
            max_steps: Some(50),
            ..Default::default()
        };
        
        let initial_state = WorkflowState {
            user_request: request.to_string(),
            analysis: None,
            approval_needed: false,
            approved: false,
            result: None,
            step_count: 0,
        };
        
        println!("💭 Initial request: {}", request);
        
        // Stream execution to see intermediate steps
        let mut stream = compiled_graph.stream_with_config(initial_state, config).await?;
        let mut final_state = None;
        
        while let Some(event) = stream.next().await {
            match event.event_type {
                StreamEventType::NodeStart => {
                    if let Some(node) = &event.node {
                        println!("🟡 Starting node: {}", node);
                    }
                }
                StreamEventType::NodeComplete => {
                    if let Some(node) = &event.node {
                        println!("✅ Completed node: {}", node);
                    }
                }
                StreamEventType::StateUpdate => {
                    if let StreamEventData::State(state) = &event.data {
                        println!("📝 State update - Step: {}", state.step_count);
                        if let Some(analysis) = &state.analysis {
                            println!("   Analysis: {}", analysis);
                        }
                        if state.approval_needed {
                            println!("   ⚠️  Approval required");
                        }
                    }
                }
                StreamEventType::GraphComplete => {
                    if let StreamEventData::State(state) = event.data {
                        final_state = Some(state);
                    }
                    break;
                }
                _ => {}
            }
        }
        
        if let Some(final_state) = final_state {
            println!("\n🎯 Final Result:");
            println!("  Steps taken: {}", final_state.step_count);
            println!("  Approval needed: {}", final_state.approval_needed);
            println!("  Approved: {}", final_state.approved);
            if let Some(result) = &final_state.result {
                println!("  Result: {}", result);
            }
        }
        
        // Show checkpoint information
        println!("\n💾 Checkpoint saved for thread: {}", thread_id);
    }
    
    println!("\n🎉 Advanced example completed successfully!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_workflow_approval_needed() {
        let mut graph = StateGraph::<WorkflowState>::new();
        graph.add_node("analyze", analyze_request).unwrap();
        graph.add_node("human_approval", human_approval).unwrap();
        graph.add_node("process", process_request).unwrap();
        
        graph.add_edge(START, "analyze").unwrap();
        graph.add_conditional_edge("analyze", needs_approval, 
            vec!["human_approval".to_string(), "process".to_string()]).unwrap();
        graph.add_conditional_edge("human_approval", should_process, 
            vec!["process".to_string(), END.to_string()]).unwrap();
        graph.add_edge("process", END).unwrap();
        
        let compiled = graph.compile().await.unwrap();
        
        let initial_state = WorkflowState {
            user_request: "This is an important decision".to_string(),
            analysis: None,
            approval_needed: false,
            approved: false,
            result: None,
            step_count: 0,
        };
        
        let result = compiled.invoke(initial_state).await.unwrap();
        assert!(result.approval_needed);
        assert!(result.step_count >= 3); // analyze + approval + process
    }
    
    #[tokio::test]
    async fn test_workflow_no_approval() {
        let mut graph = StateGraph::<WorkflowState>::new();
        graph.add_node("analyze", analyze_request).unwrap();
        graph.add_node("process", process_request).unwrap();
        
        graph.add_edge(START, "analyze").unwrap();
        graph.add_conditional_edge("analyze", needs_approval, 
            vec!["human_approval".to_string(), "process".to_string()]).unwrap();
        graph.add_edge("process", END).unwrap();
        
        let compiled = graph.compile().await.unwrap();
        
        let initial_state = WorkflowState {
            user_request: "Simple request".to_string(),
            analysis: None,
            approval_needed: false,
            approved: false,
            result: None,
            step_count: 0,
        };
        
        let result = compiled.invoke(initial_state).await.unwrap();
        assert!(!result.approval_needed);
        assert!(result.step_count == 2); // analyze + process only
    }
}
