//! Enhanced execution context with runtime features

use crate::runtime::LangGraphRuntime;
use langgraph_core::GraphConfig;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Enhanced execution context with runtime capabilities
#[derive(Debug, Clone)]
pub struct RuntimeContext {
    pub config: GraphConfig,
    pub metadata: HashMap<String, String>,
    pub runtime: Option<Arc<LangGraphRuntime>>,
    pub variables: HashMap<String, ContextValue>,
}

/// Context value that can hold different types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<ContextValue>),
    Object(HashMap<String, ContextValue>),
}

impl RuntimeContext {
    /// Create a new runtime context
    pub fn new(config: GraphConfig) -> Self {
        Self {
            config,
            metadata: HashMap::new(),
            runtime: None,
            variables: HashMap::new(),
        }
    }

    /// Create context with runtime
    pub fn with_runtime(config: GraphConfig, runtime: Arc<LangGraphRuntime>) -> Self {
        Self {
            config,
            metadata: HashMap::new(),
            runtime: Some(runtime),
            variables: HashMap::new(),
        }
    }

    /// Set a context variable
    pub fn set_variable(&mut self, key: String, value: ContextValue) {
        self.variables.insert(key, value);
    }

    /// Get a context variable
    pub fn get_variable(&self, key: &str) -> Option<&ContextValue> {
        self.variables.get(key)
    }

    /// Set metadata
    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }

    /// Get metadata
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }

    /// Check if context has runtime
    pub fn has_runtime(&self) -> bool {
        self.runtime.is_some()
    }

    /// Get runtime reference
    pub fn runtime(&self) -> Option<&Arc<LangGraphRuntime>> {
        self.runtime.as_ref()
    }

    /// Convert to basic execution context
    pub fn to_execution_context(&self) -> langgraph_core::ExecutionContext {
        langgraph_core::ExecutionContext {
            execution_id: Uuid::new_v4(),
            step: 0,
            node_name: String::new(),
            graph_name: String::new(),
            config: self
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect(),
            metadata: HashMap::new(),
            checkpoint_id: None,
            thread_id: self.config.thread_id.clone(),
        }
    }
}

impl From<langgraph_core::ExecutionContext> for RuntimeContext {
    fn from(ctx: langgraph_core::ExecutionContext) -> Self {
        Self {
            config: GraphConfig::default(),
            metadata: ctx
                .metadata
                .iter()
                .filter_map(|(k, v)| {
                    if let serde_json::Value::String(s) = v {
                        Some((k.clone(), s.clone()))
                    } else {
                        None
                    }
                })
                .collect(),
            runtime: None,
            variables: HashMap::new(),
        }
    }
}

/// Context builder for easier creation
pub struct ContextBuilder {
    config: GraphConfig,
    metadata: HashMap<String, String>,
    runtime: Option<Arc<LangGraphRuntime>>,
    variables: HashMap<String, ContextValue>,
}

impl ContextBuilder {
    /// Create a new context builder
    pub fn new() -> Self {
        Self {
            config: GraphConfig::default(),
            metadata: HashMap::new(),
            runtime: None,
            variables: HashMap::new(),
        }
    }

    /// Set execution config
    pub fn config(mut self, config: GraphConfig) -> Self {
        self.config = config;
        self
    }

    /// Set thread ID
    pub fn thread_id(mut self, thread_id: String) -> Self {
        self.config.thread_id = Some(thread_id);
        self
    }

    /// Set max steps
    pub fn max_steps(mut self, limit: u32) -> Self {
        self.config.max_steps = Some(limit);
        self
    }

    /// Add metadata
    pub fn metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set runtime
    pub fn runtime(mut self, runtime: Arc<LangGraphRuntime>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    /// Add variable
    pub fn variable(mut self, key: String, value: ContextValue) -> Self {
        self.variables.insert(key, value);
        self
    }

    /// Build the context
    pub fn build(self) -> RuntimeContext {
        RuntimeContext {
            config: self.config,
            metadata: self.metadata,
            runtime: self.runtime,
            variables: self.variables,
        }
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_builder() {
        let context = ContextBuilder::new()
            .thread_id("test-thread".to_string())
            .max_steps(100)
            .metadata("key".to_string(), "value".to_string())
            .variable(
                "test_var".to_string(),
                ContextValue::String("test".to_string()),
            )
            .build();

        assert_eq!(context.config.thread_id, Some("test-thread".to_string()));
        assert_eq!(context.config.max_steps, Some(100));
        assert_eq!(context.get_metadata("key"), Some(&"value".to_string()));
        assert!(matches!(
            context.get_variable("test_var"),
            Some(ContextValue::String(_))
        ));
    }

    #[test]
    fn test_context_variables() {
        let mut context = RuntimeContext::new(GraphConfig::default());

        context.set_variable(
            "string".to_string(),
            ContextValue::String("hello".to_string()),
        );
        context.set_variable("number".to_string(), ContextValue::Integer(42));
        context.set_variable("flag".to_string(), ContextValue::Boolean(true));

        assert!(matches!(
            context.get_variable("string"),
            Some(ContextValue::String(_))
        ));
        assert!(matches!(
            context.get_variable("number"),
            Some(ContextValue::Integer(42))
        ));
        assert!(matches!(
            context.get_variable("flag"),
            Some(ContextValue::Boolean(true))
        ));
    }
}
