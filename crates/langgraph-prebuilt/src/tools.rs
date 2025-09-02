//! Common tools for agents

use crate::agents::Tool;
use serde_json::{json, Value};
use std::process::Command;

/// Calculator tool for basic math operations
pub struct CalculatorTool;

#[async_trait::async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }
    
    fn description(&self) -> &str {
        "Perform basic math calculations. Input should be a mathematical expression as a string."
    }
    
    async fn call(&self, input: Value) -> Result<Value, String> {
        let expression = input.as_str()
            .ok_or("Input must be a string")?;
        
        // Simple math parser - in reality you'd use a proper math library
        let result = match expression {
            expr if expr.contains('+') => {
                let parts: Vec<&str> = expr.split('+').collect();
                if parts.len() == 2 {
                    let a: f64 = parts[0].trim().parse().map_err(|_| "Invalid number")?;
                    let b: f64 = parts[1].trim().parse().map_err(|_| "Invalid number")?;
                    a + b
                } else {
                    return Err("Invalid addition expression".to_string());
                }
            },
            expr if expr.contains('-') => {
                let parts: Vec<&str> = expr.split('-').collect();
                if parts.len() == 2 {
                    let a: f64 = parts[0].trim().parse().map_err(|_| "Invalid number")?;
                    let b: f64 = parts[1].trim().parse().map_err(|_| "Invalid number")?;
                    a - b
                } else {
                    return Err("Invalid subtraction expression".to_string());
                }
            },
            expr if expr.contains('*') => {
                let parts: Vec<&str> = expr.split('*').collect();
                if parts.len() == 2 {
                    let a: f64 = parts[0].trim().parse().map_err(|_| "Invalid number")?;
                    let b: f64 = parts[1].trim().parse().map_err(|_| "Invalid number")?;
                    a * b
                } else {
                    return Err("Invalid multiplication expression".to_string());
                }
            },
            expr if expr.contains('/') => {
                let parts: Vec<&str> = expr.split('/').collect();
                if parts.len() == 2 {
                    let a: f64 = parts[0].trim().parse().map_err(|_| "Invalid number")?;
                    let b: f64 = parts[1].trim().parse().map_err(|_| "Invalid number")?;
                    if b == 0.0 {
                        return Err("Division by zero".to_string());
                    }
                    a / b
                } else {
                    return Err("Invalid division expression".to_string());
                }
            },
            _ => return Err("Unsupported operation".to_string()),
        };
        
        Ok(json!({
            "expression": expression,
            "result": result
        }))
    }
}

/// Shell command execution tool (use with caution!)
pub struct ShellTool {
    allowed_commands: Vec<String>,
}

impl ShellTool {
    pub fn new() -> Self {
        Self {
            allowed_commands: vec![
                "ls".to_string(),
                "pwd".to_string(),
                "date".to_string(),
                "whoami".to_string(),
            ],
        }
    }
    
    pub fn with_allowed_commands(allowed: Vec<String>) -> Self {
        Self {
            allowed_commands: allowed,
        }
    }
}

#[async_trait::async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }
    
    fn description(&self) -> &str {
        "Execute shell commands. Only safe, read-only commands are allowed."
    }
    
    async fn call(&self, input: Value) -> Result<Value, String> {
        let command_str = input.as_str()
            .ok_or("Input must be a string")?;
        
        let command_parts: Vec<&str> = command_str.split_whitespace().collect();
        if command_parts.is_empty() {
            return Err("Empty command".to_string());
        }
        
        let command_name = command_parts[0];
        if !self.allowed_commands.contains(&command_name.to_string()) {
            return Err(format!("Command '{}' is not allowed", command_name));
        }
        
        let output = Command::new(command_name)
            .args(&command_parts[1..])
            .output()
            .map_err(|e| format!("Failed to execute command: {}", e))?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        Ok(json!({
            "command": command_str,
            "stdout": stdout.trim(),
            "stderr": stderr.trim(),
            "exit_code": output.status.code()
        }))
    }
}

/// Web search tool (mock implementation)
pub struct WebSearchTool {
    api_key: Option<String>,
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self { api_key: None }
    }
    
    pub fn with_api_key(api_key: String) -> Self {
        Self { api_key: Some(api_key) }
    }
}

#[async_trait::async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    
    fn description(&self) -> &str {
        "Search the web for information. Input should be a search query string."
    }
    
    async fn call(&self, input: Value) -> Result<Value, String> {
        let query = input.as_str()
            .ok_or("Input must be a search query string")?;
        
        // Mock search results - in reality you'd call a search API
        let mock_results = vec![
            json!({
                "title": format!("Result 1 for '{}'", query),
                "url": "https://example.com/1",
                "snippet": "This is a mock search result snippet..."
            }),
            json!({
                "title": format!("Result 2 for '{}'", query),
                "url": "https://example.com/2",
                "snippet": "Another mock search result with relevant information..."
            }),
        ];
        
        Ok(json!({
            "query": query,
            "results": mock_results,
            "total_results": 2
        }))
    }
}

/// File system tool for reading files
pub struct FileSystemTool {
    allowed_paths: Vec<String>,
}

impl FileSystemTool {
    pub fn new() -> Self {
        Self {
            allowed_paths: vec![],
        }
    }
    
    pub fn with_allowed_paths(paths: Vec<String>) -> Self {
        Self {
            allowed_paths: paths,
        }
    }
}

#[async_trait::async_trait]
impl Tool for FileSystemTool {
    fn name(&self) -> &str {
        "filesystem"
    }
    
    fn description(&self) -> &str {
        "Read files from the filesystem. Input should be a file path."
    }
    
    async fn call(&self, input: Value) -> Result<Value, String> {
        let file_path = input.as_str()
            .ok_or("Input must be a file path string")?;
        
        // Check if path is allowed
        if !self.allowed_paths.is_empty() {
            let allowed = self.allowed_paths.iter()
                .any(|allowed_path| file_path.starts_with(allowed_path));
            
            if !allowed {
                return Err(format!("Access to path '{}' is not allowed", file_path));
            }
        }
        
        match std::fs::read_to_string(file_path) {
            Ok(content) => Ok(json!({
                "path": file_path,
                "content": content,
                "size": content.len()
            })),
            Err(e) => Err(format!("Failed to read file '{}': {}", file_path, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_calculator_tool() {
        let calc = CalculatorTool;
        
        let result = calc.call(json!("2 + 3")).await.unwrap();
        assert_eq!(result["result"], 5.0);
        
        let result = calc.call(json!("10 - 4")).await.unwrap();
        assert_eq!(result["result"], 6.0);
        
        let result = calc.call(json!("3 * 4")).await.unwrap();
        assert_eq!(result["result"], 12.0);
        
        let result = calc.call(json!("8 / 2")).await.unwrap();
        assert_eq!(result["result"], 4.0);
    }
    
    #[tokio::test]
    async fn test_shell_tool() {
        let shell = ShellTool::new();
        
        let result = shell.call(json!("pwd")).await.unwrap();
        assert!(result["stdout"].as_str().unwrap().len() > 0);
        
        // Should fail for disallowed command
        let result = shell.call(json!("rm -rf /")).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_web_search_tool() {
        let search = WebSearchTool::new();
        
        let result = search.call(json!("rust programming")).await.unwrap();
        assert_eq!(result["query"], "rust programming");
        assert_eq!(result["total_results"], 2);
    }
}
