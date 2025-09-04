//! Prebuilt agent implementations

use langgraph_core::{
    CompiledGraph, ExecutionContext, GraphResult, LangGraphError, StateGraph, END, START,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

/// Standard agent state for ReAct-style agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub messages: Vec<AgentMessage>,
    pub current_thought: Option<String>,
    pub next_action: Option<AgentAction>,
    pub tool_calls: Vec<ToolCall>,
    pub intermediate_steps: Vec<AgentStep>,
    pub is_final: bool,
}

/// Message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: MessageRole,
    pub content: String,
    pub metadata: HashMap<String, String>,
}

/// Role of the message sender
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    Human,
    Assistant,
    System,
    Tool,
}

/// Action the agent wants to take
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    pub tool: String,
    pub tool_input: serde_json::Value,
    pub log: String,
}

/// Tool call and its result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub tool: String,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Intermediate step in agent reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    pub action: AgentAction,
    pub observation: String,
}

/// Tool trait for agent tools
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn call(&self, input: serde_json::Value) -> Result<serde_json::Value, String>;
}

/// LLM trait for language model integration
#[async_trait::async_trait]
pub trait LLM: Send + Sync {
    async fn generate(&self, messages: &[AgentMessage]) -> Result<String, String>;
    async fn generate_with_tools(
        &self,
        messages: &[AgentMessage],
        tools: &[&dyn Tool],
    ) -> Result<AgentResponse, String>;
}

/// Response from LLM with potential tool calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub is_final: bool,
}

/// Builder for creating ReAct agents
pub struct ReactAgentBuilder {
    llm: Option<Box<dyn LLM>>,
    tools: Vec<Box<dyn Tool>>,
    system_prompt: Option<String>,
    max_iterations: u32,
    memory_enabled: bool,
}

impl ReactAgentBuilder {
    /// Create a new React agent builder
    pub fn new() -> Self {
        Self {
            llm: None,
            tools: Vec::new(),
            system_prompt: None,
            max_iterations: 10,
            memory_enabled: false,
        }
    }

    /// Set the language model
    pub fn llm(mut self, llm: Box<dyn LLM>) -> Self {
        self.llm = Some(llm);
        self
    }

    /// Add a tool
    pub fn tool(mut self, tool: Box<dyn Tool>) -> Self {
        self.tools.push(tool);
        self
    }

    /// Add multiple tools
    pub fn tools(mut self, tools: Vec<Box<dyn Tool>>) -> Self {
        self.tools.extend(tools);
        self
    }

    /// Set system prompt
    pub fn system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    /// Set maximum iterations
    pub fn max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }

    /// Enable memory/checkpointing
    pub fn with_memory(mut self) -> Self {
        self.memory_enabled = true;
        self
    }

    /// Build the agent graph
    pub async fn build(self) -> GraphResult<CompiledGraph<AgentState>> {
        if self.llm.is_none() {
            return Err(LangGraphError::configuration("LLM is required"));
        }

        if self.tools.is_empty() {
            return Err(LangGraphError::configuration(
                "At least one tool is required",
            ));
        }

        let mut graph = StateGraph::<AgentState>::new();

        // Add system prompt if provided
        if let Some(prompt) = &self.system_prompt {
            graph.set_metadata("system_prompt", prompt)?;
        }

        graph.set_metadata("max_iterations", &self.max_iterations.to_string())?;

        // Create the agent nodes
        let llm = self.llm.unwrap();
        let tools = self.tools;

        // Reasoning node
        graph.add_node("reasoning", create_reasoning_node(llm, tools))?;

        // Tool execution node
        graph.add_node("tool_execution", tool_execution_node)?;

        // Add edges
        graph.add_edge(START, "reasoning")?;
        graph.add_conditional_edge(
            "reasoning",
            should_continue,
            vec!["tool_execution".to_string(), END.to_string()],
        )?;
        graph.add_edge("tool_execution", "reasoning")?;

        graph.compile().await
    }
}

impl Default for ReactAgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create reasoning node with captured LLM and tools
fn create_reasoning_node(
    llm: Box<dyn LLM>,
    tools: Vec<Box<dyn Tool>>,
) -> impl Fn(
    AgentState,
    ExecutionContext,
) -> std::pin::Pin<Box<dyn Future<Output = GraphResult<AgentState>> + Send>>
       + Send
       + Sync {
    let llm_arc = Arc::new(llm);
    let tools_arc = Arc::new(tools);
    move |state: AgentState, _context: ExecutionContext| {
        let llm_clone = Arc::clone(&llm_arc);
        let tools_clone = Arc::clone(&tools_arc);
        Box::pin(
            async move { reasoning_node(state, llm_clone.as_ref().as_ref(), &tools_clone).await },
        )
    }
}

/// Reasoning node implementation
async fn reasoning_node(
    mut state: AgentState,
    llm: &dyn LLM,
    tools: &[Box<dyn Tool>],
) -> GraphResult<AgentState> {
    println!("🤔 Agent reasoning...");

    // Convert tools to trait objects for LLM
    let tool_refs: Vec<&dyn Tool> = tools.iter().map(|t| t.as_ref()).collect();

    // Generate response with tools
    let response = llm
        .generate_with_tools(&state.messages, &tool_refs)
        .await
        .map_err(|e| LangGraphError::runtime(format!("LLM error: {}", e)))?;

    // Add assistant message
    state.messages.push(AgentMessage {
        role: MessageRole::Assistant,
        content: response.content.clone(),
        metadata: HashMap::new(),
    });

    // Update state with tool calls
    state.tool_calls = response.tool_calls;
    state.is_final = response.is_final || state.tool_calls.is_empty();
    state.current_thought = Some(response.content);

    Ok(state)
}

/// Tool execution node
async fn tool_execution_node(
    mut state: AgentState,
    _context: ExecutionContext,
) -> GraphResult<AgentState> {
    println!("🔧 Executing tools...");

    let mut executed_calls = Vec::new();

    for mut tool_call in state.tool_calls.drain(..) {
        println!("  📞 Calling tool: {}", tool_call.tool);

        // In a real implementation, you would look up and execute the actual tool
        // For now, we'll simulate tool execution
        tool_call.output = Some(serde_json::json!({
            "result": format!("Mock result from {}", tool_call.tool),
            "success": true
        }));

        // Add tool result message
        state.messages.push(AgentMessage {
            role: MessageRole::Tool,
            content: format!("Tool {} executed successfully", tool_call.tool),
            metadata: HashMap::new(),
        });

        executed_calls.push(tool_call);
    }

    state.tool_calls = executed_calls;
    Ok(state)
}

/// Decide whether to continue or end
fn should_continue(state: &AgentState) -> GraphResult<String> {
    if state.is_final || state.tool_calls.is_empty() {
        Ok(END.to_string())
    } else {
        Ok("tool_execution".to_string())
    }
}

/// Convenience function to create a React agent
pub async fn create_react_agent(
    llm: Box<dyn LLM>,
    tools: Vec<Box<dyn Tool>>,
) -> GraphResult<CompiledGraph<AgentState>> {
    ReactAgentBuilder::new().llm(llm).tools(tools).build().await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockLLM;

    #[async_trait::async_trait]
    impl LLM for MockLLM {
        async fn generate(&self, _messages: &[AgentMessage]) -> Result<String, String> {
            Ok("Mock response".to_string())
        }

        async fn generate_with_tools(
            &self,
            _messages: &[AgentMessage],
            _tools: &[&dyn Tool],
        ) -> Result<AgentResponse, String> {
            Ok(AgentResponse {
                content: "I need to use a tool".to_string(),
                tool_calls: vec![],
                is_final: true,
            })
        }
    }

    struct MockTool;

    #[async_trait::async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            "mock_tool"
        }
        fn description(&self) -> &str {
            "A mock tool for testing"
        }

        async fn call(&self, _input: serde_json::Value) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({"result": "mock"}))
        }
    }

    #[tokio::test]
    async fn test_react_agent_builder() {
        let llm = Box::new(MockLLM);
        let tools = vec![Box::new(MockTool) as Box<dyn Tool>];

        let agent = ReactAgentBuilder::new()
            .llm(llm)
            .tools(tools)
            .system_prompt("You are a helpful assistant".to_string())
            .max_iterations(5)
            .build()
            .await
            .unwrap();

        let initial_state = AgentState {
            messages: vec![AgentMessage {
                role: MessageRole::Human,
                content: "Hello!".to_string(),
                metadata: HashMap::new(),
            }],
            current_thought: None,
            next_action: None,
            tool_calls: Vec::new(),
            intermediate_steps: Vec::new(),
            is_final: false,
        };

        let result = agent.invoke(initial_state).await.unwrap();
        assert!(!result.messages.is_empty());
    }
}
