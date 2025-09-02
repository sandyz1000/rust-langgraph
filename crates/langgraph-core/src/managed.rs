//! Managed values for special state handling in LangGraph

use crate::errors::{GraphResult, LangGraphError};
use crate::types::GraphState;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for managed values that require special handling
#[async_trait]
pub trait ManagedValue<T>: Send + Sync
where
    T: Clone + Send + Sync + 'static,
{
    /// Initialize the managed value
    async fn initialize(&self) -> GraphResult<()>;

    /// Update the managed value
    async fn update(&self, value: T) -> GraphResult<()>;

    /// Read the current value
    async fn read(&self) -> GraphResult<Option<T>>;

    /// Clear the managed value
    async fn clear(&self) -> GraphResult<()>;

    /// Get the managed value type name
    fn type_name(&self) -> &'static str;

    /// Serialize the value for checkpointing
    async fn serialize(&self) -> GraphResult<serde_json::Value>;

    /// Deserialize the value from checkpoint
    async fn deserialize(&self, value: serde_json::Value) -> GraphResult<()>;
}

/// Managed value for conversation memory
#[derive(Debug)]
pub struct ConversationMemory {
    /// Stored messages
    messages: Arc<RwLock<Vec<Message>>>,
    /// Maximum message count
    max_messages: Option<usize>,
    /// Memory configuration
    config: MemoryConfig,
}

/// Message in conversation memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID
    pub id: String,
    /// Message role (user, assistant, system)
    pub role: String,
    /// Message content
    pub content: String,
    /// Message timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Memory configuration
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Maximum number of messages to retain
    pub max_messages: Option<usize>,
    /// Whether to enable compression
    pub enable_compression: bool,
    /// Compression threshold (number of messages)
    pub compression_threshold: usize,
    /// Whether to persist to storage
    pub persist: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_messages: Some(1000),
            enable_compression: false,
            compression_threshold: 100,
            persist: true,
        }
    }
}

impl ConversationMemory {
    /// Create a new conversation memory
    pub fn new() -> Self {
        Self::with_config(MemoryConfig::default())
    }

    /// Create conversation memory with configuration
    pub fn with_config(config: MemoryConfig) -> Self {
        Self {
            messages: Arc::new(RwLock::new(Vec::new())),
            max_messages: config.max_messages,
            config,
        }
    }

    /// Add a message to memory
    pub async fn add_message(&self, message: Message) -> GraphResult<()> {
        let mut messages = self.messages.write().await;
        messages.push(message);

        // Enforce message limit
        if let Some(max) = self.max_messages {
            if messages.len() > max {
                let excess = messages.len() - max;
                messages.drain(0..excess);
            }
        }

        Ok(())
    }

    /// Get all messages
    pub async fn get_messages(&self) -> Vec<Message> {
        let messages = self.messages.read().await;
        messages.clone()
    }

    /// Get recent messages
    pub async fn get_recent_messages(&self, count: usize) -> Vec<Message> {
        let messages = self.messages.read().await;
        if messages.len() <= count {
            messages.clone()
        } else {
            let start = messages.len() - count;
            messages[start..].to_vec()
        }
    }

    /// Search messages by content
    pub async fn search_messages(&self, query: &str) -> Vec<Message> {
        let messages = self.messages.read().await;
        messages
            .iter()
            .filter(|msg| msg.content.to_lowercase().contains(&query.to_lowercase()))
            .cloned()
            .collect()
    }

    /// Get message count
    pub async fn message_count(&self) -> usize {
        let messages = self.messages.read().await;
        messages.len()
    }

    /// Check if memory is empty
    pub async fn is_empty(&self) -> bool {
        let messages = self.messages.read().await;
        messages.is_empty()
    }
}

impl Default for ConversationMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ManagedValue<Vec<Message>> for ConversationMemory {
    async fn initialize(&self) -> GraphResult<()> {
        // Initialize memory (could load from persistent storage)
        Ok(())
    }

    async fn update(&self, messages: Vec<Message>) -> GraphResult<()> {
        let mut mem_messages = self.messages.write().await;
        *mem_messages = messages;
        Ok(())
    }

    async fn read(&self) -> GraphResult<Option<Vec<Message>>> {
        let messages = self.messages.read().await;
        Ok(Some(messages.clone()))
    }

    async fn clear(&self) -> GraphResult<()> {
        let mut messages = self.messages.write().await;
        messages.clear();
        Ok(())
    }

    fn type_name(&self) -> &'static str {
        "ConversationMemory"
    }

    async fn serialize(&self) -> GraphResult<serde_json::Value> {
        let messages = self.messages.read().await;
        Ok(serde_json::to_value(&*messages)?)
    }

    async fn deserialize(&self, value: serde_json::Value) -> GraphResult<()> {
        let messages: Vec<Message> = serde_json::from_value(value)?;
        let mut mem_messages = self.messages.write().await;
        *mem_messages = messages;
        Ok(())
    }
}

/// Managed value for caching expensive computations
#[derive(Debug)]
pub struct ComputationCache<K, V>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Cache storage
    cache: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,
    /// Cache configuration
    config: CacheConfig,
}

/// Cache entry with metadata
#[derive(Debug, Clone)]
struct CacheEntry<V> {
    /// Cached value
    value: V,
    /// Creation timestamp
    created_at: chrono::DateTime<chrono::Utc>,
    /// Last access timestamp
    accessed_at: chrono::DateTime<chrono::Utc>,
    /// Access count
    access_count: u64,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum cache size
    pub max_size: usize,
    /// Time-to-live in seconds
    pub ttl_seconds: Option<u64>,
    /// Enable LRU eviction
    pub enable_lru: bool,
    /// Enable statistics tracking
    pub enable_stats: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,
            ttl_seconds: Some(3600), // 1 hour
            enable_lru: true,
            enable_stats: true,
        }
    }
}

impl<K, V> ComputationCache<K, V>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Create a new computation cache
    pub fn new() -> Self {
        Self::with_config(CacheConfig::default())
    }

    /// Create cache with configuration
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get a value from cache
    pub async fn get(&self, key: &K) -> Option<V> {
        let mut cache = self.cache.write().await;
        
        if let Some(entry) = cache.get_mut(key) {
            // Check TTL
            if let Some(ttl) = self.config.ttl_seconds {
                let age = chrono::Utc::now() - entry.created_at;
                if age.num_seconds() as u64 > ttl {
                    cache.remove(key);
                    return None;
                }
            }

            // Update access info
            entry.accessed_at = chrono::Utc::now();
            entry.access_count += 1;
            
            Some(entry.value.clone())
        } else {
            None
        }
    }

    /// Put a value in cache
    pub async fn put(&self, key: K, value: V) -> GraphResult<()> {
        let mut cache = self.cache.write().await;

        // Check if we need to evict
        if cache.len() >= self.config.max_size && !cache.contains_key(&key) {
            self.evict_one(&mut cache).await?;
        }

        // Insert new entry
        let now = chrono::Utc::now();
        let entry = CacheEntry {
            value,
            created_at: now,
            accessed_at: now,
            access_count: 1,
        };
        
        cache.insert(key, entry);
        Ok(())
    }

    /// Remove a value from cache
    pub async fn remove(&self, key: &K) -> Option<V> {
        let mut cache = self.cache.write().await;
        cache.remove(key).map(|entry| entry.value)
    }

    /// Clear the cache
    pub async fn clear(&self) -> GraphResult<()> {
        let mut cache = self.cache.write().await;
        cache.clear();
        Ok(())
    }

    /// Get cache size
    pub async fn size(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }

    /// Check if cache contains key
    pub async fn contains(&self, key: &K) -> bool {
        let cache = self.cache.read().await;
        cache.contains_key(key)
    }

    /// Evict one entry using LRU policy
    async fn evict_one(&self, cache: &mut HashMap<K, CacheEntry<V>>) -> GraphResult<()> {
        if cache.is_empty() {
            return Ok(());
        }

        // Find LRU entry
        let mut oldest_key = None;
        let mut oldest_time = chrono::Utc::now();

        for (key, entry) in cache.iter() {
            if entry.accessed_at < oldest_time {
                oldest_time = entry.accessed_at;
                oldest_key = Some(key.clone());
            }
        }

        if let Some(key) = oldest_key {
            cache.remove(&key);
        }

        Ok(())
    }
}

impl<K, V> Default for ComputationCache<K, V>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Manager for all managed values in a graph
pub struct ManagedValueManager {
    /// Registered managed values
    values: Arc<RwLock<HashMap<String, Arc<dyn ManagedValueTrait>>>>,
}

/// Trait object for managed values
trait ManagedValueTrait: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
    fn type_name(&self) -> &'static str;
}

impl<T> ManagedValueTrait for T
where
    T: ManagedValue<serde_json::Value> + 'static,
{
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn type_name(&self) -> &'static str {
        ManagedValue::type_name(self)
    }
}

impl ManagedValueManager {
    /// Create a new managed value manager
    pub fn new() -> Self {
        Self {
            values: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a managed value
    pub async fn register<T>(&self, name: String, value: T) -> GraphResult<()>
    where
        T: ManagedValueTrait + 'static,
    {
        let mut values = self.values.write().await;
        values.insert(name, Arc::new(value));
        Ok(())
    }

    /// Get a managed value by name
    pub async fn get<T>(&self, name: &str) -> Option<Arc<T>>
    where
        T: ManagedValueTrait + 'static,
    {
        let values = self.values.read().await;
        values.get(name).and_then(|value| {
            value.as_any().downcast_ref::<T>().map(|v| {
                // This is a bit of a hack, but necessary for the type system
                unsafe { Arc::from_raw(v as *const T) }
            })
        })
    }

    /// List all managed value names
    pub async fn list_names(&self) -> Vec<String> {
        let values = self.values.read().await;
        values.keys().cloned().collect()
    }

    /// Remove a managed value
    pub async fn remove(&self, name: &str) -> GraphResult<bool> {
        let mut values = self.values.write().await;
        Ok(values.remove(name).is_some())
    }

    /// Clear all managed values
    pub async fn clear(&self) -> GraphResult<()> {
        let mut values = self.values.write().await;
        values.clear();
        Ok(())
    }

    /// Get manager statistics
    pub async fn stats(&self) -> ManagedValueStats {
        let values = self.values.read().await;
        let mut type_counts = HashMap::new();
        
        for value in values.values() {
            let type_name = value.type_name();
            *type_counts.entry(type_name.to_string()).or_insert(0) += 1;
        }

        ManagedValueStats {
            total_count: values.len(),
            type_counts,
        }
    }
}

impl Default for ManagedValueManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for managed values
#[derive(Debug, Clone)]
pub struct ManagedValueStats {
    /// Total number of managed values
    pub total_count: usize,
    /// Count by type
    pub type_counts: HashMap<String, usize>,
}

/// Helper function to create a conversation memory managed value
pub fn create_conversation_memory() -> ConversationMemory {
    ConversationMemory::new()
}

/// Helper function to create a computation cache managed value
pub fn create_computation_cache<K, V>() -> ComputationCache<K, V>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    ComputationCache::new()
}
