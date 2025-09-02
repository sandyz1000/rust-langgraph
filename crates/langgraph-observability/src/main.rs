use langgraph_observability::{
    dashboard::Dashboard,
    config::DashboardConfig,
    storage::InMemoryStorage,
    events::EventBus,
};
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create configuration
    let config = DashboardConfig {
        enabled: true,
        host: "127.0.0.1".to_string(),
        port: 3000,
        static_assets_path: Some("static".to_string()),
        websocket_enabled: true,
        auth: None,
    };

    // Create storage backend
    let storage = InMemoryStorage::new();
    let storage = Arc::new(RwLock::new(storage));
    
    // Create event bus
    let event_bus = EventBus::new();

    // Create and start dashboard server
    let dashboard = Dashboard::new(config, storage, event_bus);
    
    println!("🚀 LangGraph Observatory Dashboard starting on http://127.0.0.1:3000");
    
    dashboard.start("127.0.0.1:3000").await?;

    Ok(())
}
