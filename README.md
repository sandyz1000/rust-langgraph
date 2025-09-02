# LangGraph Rust Implementation

A comprehensive Rust implementation of LangGraph, a library for building stateful, multi-actor applications with LLMs. This implementation provides the same core functionality as the original Python version with Rust's performance, safety, and concurrency benefits.

## 🚀 Features

- **Stateful Graph Orchestration**: Build complex, stateful applications using a graph-based approach
- **Async/Await Support**: Full async ecosystem integration with Tokio
- **Checkpointing**: Persistent state management with multiple storage backends
- **Streaming**: Real-time execution with event streaming
- **Human-in-the-Loop**: Built-in support for human approval workflows
- **Type Safety**: Leverage Rust's type system for reliable graph execution
- **Concurrent Execution**: Efficient parallel node execution
- **Flexible Serialization**: Multiple serialization protocols (JSON, MessagePack, compression)

## 📦 Project Structure

```
rust-langgraph/
├── Cargo.toml                 # Workspace configuration
├── README.md                  # This file
├── crates/
│   ├── langgraph-core/        # Core graph functionality
│   ├── langgraph-checkpoint/  # Checkpointing system
│   ├── langgraph-runtime/     # Runtime and execution context
│   ├── langgraph-prebuilt/    # High-level agent builders
│   └── langgraph-cli/         # Command-line interface
└── examples/                  # Example applications
    ├── basic_agent.rs         # Simple agent example
    ├── advanced_workflow.rs   # Complex workflow with checkpointing
    └── streaming.rs           # Real-time streaming example
```

## 🏗️ Architecture

### Core Components

1. **StateGraph**: The main graph building interface
2. **Pregel Engine**: Distributed graph computation engine inspired by Google's Pregel
3. **Channels**: Communication system between nodes
4. **Checkpointing**: State persistence and recovery
5. **Streaming**: Real-time event emission and processing

### Key Types

- `GraphState`: Trait for defining application state
- `NodeFunction`: Async trait for node implementations  
- `ExecutionContext`: Runtime context with configuration and metadata
- `StreamEvent`: Event types for real-time updates

## 🚀 Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
rust-langgraph = { path = "path/to/rust-langgraph" }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
```

### Basic Example

```rust
use rust_langgraph::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    messages: Vec<String>,
    count: u32,
}

async fn my_node(state: State, _ctx: ExecutionContext) -> GraphResult<State> {
    Ok(State {
        messages: {
            let mut msgs = state.messages;
            msgs.push("Hello from node!".to_string());
            msgs
        },
        count: state.count + 1,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build graph
    let mut graph = StateGraph::<State>::new();
    graph.add_node("my_node", my_node)?;
    graph.add_edge(START, "my_node")?;
    graph.add_edge("my_node", END)?;
    
    // Compile and run
    let app = graph.compile().await?;
    let initial_state = State { messages: vec![], count: 0 };
    let result = app.invoke(initial_state).await?;
    
    println!("Final state: {:?}", result);
    Ok(())
}
```

## 📚 Examples

### 1. Basic Agent (`examples/basic_agent.rs`)
Demonstrates:
- Simple state management
- Node creation and connection
- Conditional routing
- Basic streaming

### 2. Advanced Workflow (`examples/advanced_workflow.rs`)
Demonstrates:
- Human-in-the-loop patterns
- Checkpointing
- Complex conditional logic
- Multi-step workflows

### 3. Streaming (`examples/streaming.rs`)
Demonstrates:
- Real-time event streaming
- Different streaming modes
- Token-by-token processing
- Progress monitoring

Run examples:
```bash
cargo run --example basic_agent
cargo run --example advanced_workflow
cargo run --example streaming
```

## 🔧 API Reference

### StateGraph

The main interface for building graphs:

```rust
let mut graph = StateGraph::<MyState>::new();

// Add nodes
graph.add_node("node_name", node_function)?;

// Add edges
graph.add_edge(START, "node_name")?;
graph.add_edge("node_name", END)?;
graph.add_conditional_edge("node_name", condition_fn, targets)?;

// Compile
let app = graph.compile().await?;
```

### Execution

Execute graphs in different ways:

```rust
// Simple execution
let result = app.invoke(initial_state).await?;

// Streaming execution
let mut stream = app.stream(initial_state).await?;
while let Some(event) = stream.next().await {
    // Process streaming events
}

// With configuration
let config = ExecutionConfig {
    thread_id: Some("thread-1".to_string()),
    recursion_limit: 100,
    stream_mode: StreamMode::Values,
    ..Default::default()
};
let result = app.invoke_with_config(initial_state, config).await?;
```

### Checkpointing

Persist and restore state:

```rust
use rust_langgraph::checkpoint::InMemoryCheckpointer;

let checkpointer = InMemoryCheckpointer::new();
// Use with graphs for automatic state persistence
```

## 🔄 Stream Modes

- `Values`: Stream complete state after each node
- `Updates`: Stream only state changes from each node  
- `Debug`: Stream detailed execution information

## 🧪 Testing

Run tests for all crates:

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p langgraph-core

# Run with output
cargo test -- --nocapture
```

## 🚧 Current Status

### ✅ Completed
- Core graph building and execution
- Pregel-based execution engine
- State management and type system
- Basic streaming support
- In-memory checkpointing
- Channel system for communication
- Error handling and validation
- Comprehensive examples

### 🚧 In Progress
- Additional checkpoint backends (SQLite, PostgreSQL, Redis)
- Advanced streaming features
- Performance optimizations
- Integration with LLM libraries

### 📋 Planned
- Web UI for graph visualization
- More prebuilt agent types
- Plugin system for extensions
- Monitoring and observability tools

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Original [LangGraph](https://github.com/langchain-ai/langgraph) Python implementation
- Google's Pregel paper for the distributed graph computation model
- The Rust community for excellent async ecosystem tools

## 📖 Documentation

For detailed documentation, see:
- [Core Concepts](docs/core-concepts.md)
- [API Reference](docs/api-reference.md)
- [Examples Guide](docs/examples.md)
- [Checkpoint Backends](docs/checkpointing.md)

## 💬 Community

- [Discussions](https://github.com/yourusername/rust-langgraph/discussions)
- [Issues](https://github.com/yourusername/rust-langgraph/issues)
- [Discord](https://discord.gg/rust-langgraph) (coming soon)
