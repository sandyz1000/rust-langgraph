//! Common prompt templates and utilities

use std::collections::HashMap;

/// Template for React-style agent prompts
pub struct ReactPromptTemplate {
    system_prompt: String,
    tools_description: String,
    examples: Vec<String>,
}

impl ReactPromptTemplate {
    /// Create a new React prompt template
    pub fn new() -> Self {
        Self {
            system_prompt: DEFAULT_REACT_SYSTEM_PROMPT.to_string(),
            tools_description: String::new(),
            examples: Vec::new(),
        }
    }
    
    /// Set custom system prompt
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = prompt;
        self
    }
    
    /// Set tools description
    pub fn with_tools_description(mut self, description: String) -> Self {
        self.tools_description = description;
        self
    }
    
    /// Add example
    pub fn with_example(mut self, example: String) -> Self {
        self.examples.push(example);
        self
    }
    
    /// Build the complete prompt
    pub fn build(&self) -> String {
        let mut prompt = self.system_prompt.clone();
        
        if !self.tools_description.is_empty() {
            prompt.push_str("\n\nAvailable tools:\n");
            prompt.push_str(&self.tools_description);
        }
        
        if !self.examples.is_empty() {
            prompt.push_str("\n\nExamples:\n");
            for example in &self.examples {
                prompt.push_str(example);
                prompt.push_str("\n");
            }
        }
        
        prompt.push_str(REACT_FORMAT_INSTRUCTIONS);
        prompt
    }
}

impl Default for ReactPromptTemplate {
    fn default() -> Self {
        Self::new()
    }
}

/// Default React system prompt
const DEFAULT_REACT_SYSTEM_PROMPT: &str = r#"You are a helpful assistant that can use tools to help answer questions and solve problems.

You should use the following process:
1. Think about the question or problem
2. Determine if you need to use any tools
3. If tools are needed, use them to gather information
4. Provide a final answer based on your reasoning and any tool results"#;

/// React format instructions
const REACT_FORMAT_INSTRUCTIONS: &str = r#"

Use the following format:

Thought: Think about what you need to do
Action: The action to take (if needed)
Action Input: The input to the action
Observation: The result of the action
... (this Thought/Action/Action Input/Observation can repeat)
Thought: I now know the final answer
Final Answer: The final answer to the original question"#;

/// Template for chat-based agents
pub struct ChatPromptTemplate {
    system_prompt: String,
    variables: HashMap<String, String>,
}

impl ChatPromptTemplate {
    /// Create new chat prompt template
    pub fn new(system_prompt: String) -> Self {
        Self {
            system_prompt,
            variables: HashMap::new(),
        }
    }
    
    /// Add a variable
    pub fn with_variable(mut self, key: String, value: String) -> Self {
        self.variables.insert(key, value);
        self
    }
    
    /// Format the prompt with variables
    pub fn format(&self) -> String {
        let mut prompt = self.system_prompt.clone();
        
        for (key, value) in &self.variables {
            let placeholder = format!("{{{}}}", key);
            prompt = prompt.replace(&placeholder, value);
        }
        
        prompt
    }
}

/// Common prompt templates
pub struct PromptTemplates;

impl PromptTemplates {
    /// React agent prompt
    pub fn react_agent() -> ReactPromptTemplate {
        ReactPromptTemplate::new()
    }
    
    /// Chat assistant prompt
    pub fn chat_assistant() -> ChatPromptTemplate {
        ChatPromptTemplate::new(
            "You are a helpful AI assistant. Be concise, accurate, and helpful in your responses.".to_string()
        )
    }
    
    /// Code assistant prompt
    pub fn code_assistant() -> ChatPromptTemplate {
        ChatPromptTemplate::new(
            r#"You are an expert software engineer and coding assistant. 
You help with:
- Writing clean, efficient code
- Debugging and troubleshooting
- Code reviews and best practices
- Architecture and design decisions

Always provide working code examples and explain your reasoning."#.to_string()
        )
    }
    
    /// Analysis agent prompt
    pub fn analysis_agent() -> ChatPromptTemplate {
        ChatPromptTemplate::new(
            r#"You are a data analysis expert. Your role is to:
- Analyze data and identify patterns
- Provide insights and recommendations
- Create clear, actionable reports
- Explain complex findings in simple terms

Be thorough in your analysis and always support your conclusions with evidence."#.to_string()
        )
    }
    
    /// Research assistant prompt
    pub fn research_assistant() -> ReactPromptTemplate {
        ReactPromptTemplate::new()
            .with_system_prompt(
                r#"You are a research assistant that helps find, analyze, and synthesize information.
Your strengths include:
- Thorough web research
- Critical evaluation of sources
- Clear summarization of findings
- Identifying reliable and authoritative sources

Always verify information from multiple sources when possible."#.to_string()
            )
    }
}

/// Prompt formatting utilities
pub struct PromptFormatter;

impl PromptFormatter {
    /// Format a prompt with variables
    pub fn format(template: &str, variables: &HashMap<String, String>) -> String {
        let mut result = template.to_string();
        
        for (key, value) in variables {
            let placeholder = format!("{{{}}}", key);
            result = result.replace(&placeholder, value);
        }
        
        result
    }
    
    /// Create a conversation prompt from messages
    pub fn conversation_prompt(messages: &[crate::agents::AgentMessage]) -> String {
        let mut prompt = String::new();
        
        for message in messages {
            match message.role {
                crate::agents::MessageRole::Human => {
                    prompt.push_str("Human: ");
                    prompt.push_str(&message.content);
                    prompt.push('\n');
                }
                crate::agents::MessageRole::Assistant => {
                    prompt.push_str("Assistant: ");
                    prompt.push_str(&message.content);
                    prompt.push('\n');
                }
                crate::agents::MessageRole::System => {
                    prompt.push_str("System: ");
                    prompt.push_str(&message.content);
                    prompt.push('\n');
                }
                crate::agents::MessageRole::Tool => {
                    prompt.push_str("Tool: ");
                    prompt.push_str(&message.content);
                    prompt.push('\n');
                }
            }
        }
        
        prompt.push_str("Assistant: ");
        prompt
    }
    
    /// Extract variables from a template
    pub fn extract_variables(template: &str) -> Vec<String> {
        let mut variables = Vec::new();
        let mut chars = template.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '{' {
                let mut var_name = String::new();
                while let Some(&next_char) = chars.peek() {
                    if next_char == '}' {
                        chars.next(); // consume '}'
                        break;
                    }
                    var_name.push(chars.next().unwrap());
                }
                if !var_name.is_empty() && !variables.contains(&var_name) {
                    variables.push(var_name);
                }
            }
        }
        
        variables
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_react_prompt_template() {
        let template = ReactPromptTemplate::new()
            .with_tools_description("calculator: Do math calculations".to_string())
            .with_example("Q: What is 2+2? A: Let me calculate that.".to_string());
        
        let prompt = template.build();
        assert!(prompt.contains("helpful assistant"));
        assert!(prompt.contains("calculator: Do math calculations"));
        assert!(prompt.contains("Q: What is 2+2?"));
    }
    
    #[test]
    fn test_chat_prompt_template() {
        let template = ChatPromptTemplate::new("Hello {name}, you are a {role}".to_string())
            .with_variable("name".to_string(), "Alice".to_string())
            .with_variable("role".to_string(), "developer".to_string());
        
        let formatted = template.format();
        assert_eq!(formatted, "Hello Alice, you are a developer");
    }
    
    #[test]
    fn test_prompt_formatter() {
        let template = "The {animal} is {color}";
        let mut variables = HashMap::new();
        variables.insert("animal".to_string(), "cat".to_string());
        variables.insert("color".to_string(), "black".to_string());
        
        let result = PromptFormatter::format(template, &variables);
        assert_eq!(result, "The cat is black");
    }
    
    #[test]
    fn test_extract_variables() {
        let template = "Hello {name}, your {item} is {color}";
        let variables = PromptFormatter::extract_variables(template);
        
        assert_eq!(variables.len(), 3);
        assert!(variables.contains(&"name".to_string()));
        assert!(variables.contains(&"item".to_string()));
        assert!(variables.contains(&"color".to_string()));
    }
}
