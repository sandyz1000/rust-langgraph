//! Channel system for inter-node communication

use crate::errors::{GraphResult, LangGraphError};
use crate::types::GraphState;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, watch, RwLock};
use uuid::Uuid;

/// Trait for communication channels between nodes
#[async_trait]
pub trait Channel<T>: Send + Sync
where
    T: Clone + Send + Sync + 'static,
{
    /// Write a value to the channel
    async fn write(&self, value: T) -> GraphResult<()>;

    /// Read the current value from the channel
    async fn read(&self) -> GraphResult<Option<T>>;

    /// Get the channel version/revision
    async fn version(&self) -> GraphResult<u64>;

    /// Clear the channel
    async fn clear(&self) -> GraphResult<()>;

    /// Check if the channel has been updated since a given version
    async fn is_updated(&self, since_version: u64) -> GraphResult<bool>;
}

/// Last-value channel that stores the most recent value
#[derive(Debug, Clone)]
pub struct LastValueChannel<T> {
    value: Arc<RwLock<Option<T>>>,
    version: Arc<RwLock<u64>>,
    sender: broadcast::Sender<T>,
}

impl<T> LastValueChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    /// Create a new last-value channel
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self {
            value: Arc::new(RwLock::new(None)),
            version: Arc::new(RwLock::new(0)),
            sender,
        }
    }

    /// Subscribe to value updates
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.sender.subscribe()
    }
}

impl<T> Default for LastValueChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<T> Channel<T> for LastValueChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    async fn write(&self, value: T) -> GraphResult<()> {
        {
            let mut val = self.value.write().await;
            *val = Some(value.clone());
        }
        {
            let mut ver = self.version.write().await;
            *ver += 1;
        }
        // Ignore send errors (no active receivers)
        let _ = self.sender.send(value);
        Ok(())
    }

    async fn read(&self) -> GraphResult<Option<T>> {
        let value = self.value.read().await;
        Ok(value.clone())
    }

    async fn version(&self) -> GraphResult<u64> {
        let version = self.version.read().await;
        Ok(*version)
    }

    async fn clear(&self) -> GraphResult<()> {
        {
            let mut val = self.value.write().await;
            *val = None;
        }
        {
            let mut ver = self.version.write().await;
            *ver += 1;
        }
        Ok(())
    }

    async fn is_updated(&self, since_version: u64) -> GraphResult<bool> {
        let current_version = self.version().await?;
        Ok(current_version > since_version)
    }
}

/// Accumulator channel that collects values into a list
#[derive(Debug)]
pub struct AccumulatorChannel<T> {
    values: Arc<RwLock<Vec<T>>>,
    version: Arc<RwLock<u64>>,
    sender: broadcast::Sender<Vec<T>>,
}

impl<T> AccumulatorChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    /// Create a new accumulator channel
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self {
            values: Arc::new(RwLock::new(Vec::new())),
            version: Arc::new(RwLock::new(0)),
            sender,
        }
    }

    /// Subscribe to value updates
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<T>> {
        self.sender.subscribe()
    }
}

impl<T> Default for AccumulatorChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<T> Channel<Vec<T>> for AccumulatorChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    async fn write(&self, value: Vec<T>) -> GraphResult<()> {
        {
            let mut vals = self.values.write().await;
            vals.extend(value.clone());
        }
        {
            let mut ver = self.version.write().await;
            *ver += 1;
        }
        let current_values = {
            let vals = self.values.read().await;
            vals.clone()
        };
        // Ignore send errors (no active receivers)
        let _ = self.sender.send(current_values);
        Ok(())
    }

    async fn read(&self) -> GraphResult<Option<Vec<T>>> {
        let values = self.values.read().await;
        Ok(Some(values.clone()))
    }

    async fn version(&self) -> GraphResult<u64> {
        let version = self.version.read().await;
        Ok(*version)
    }

    async fn clear(&self) -> GraphResult<()> {
        {
            let mut vals = self.values.write().await;
            vals.clear();
        }
        {
            let mut ver = self.version.write().await;
            *ver += 1;
        }
        Ok(())
    }

    async fn is_updated(&self, since_version: u64) -> GraphResult<bool> {
        let current_version = self.version().await?;
        Ok(current_version > since_version)
    }
}

/// Binary operator channel that combines values using a reducer function
pub struct BinaryOpChannel<T> {
    value: Arc<RwLock<Option<T>>>,
    version: Arc<RwLock<u64>>,
    reducer: Arc<dyn Fn(T, T) -> T + Send + Sync>,
    sender: broadcast::Sender<T>,
}

impl<T> BinaryOpChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    /// Create a new binary operator channel with a reducer function
    pub fn new<F>(reducer: F) -> Self
    where
        F: Fn(T, T) -> T + Send + Sync + 'static,
    {
        let (sender, _) = broadcast::channel(1000);
        Self {
            value: Arc::new(RwLock::new(None)),
            version: Arc::new(RwLock::new(0)),
            reducer: Arc::new(reducer),
            sender,
        }
    }

    /// Subscribe to value updates
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.sender.subscribe()
    }
}

#[async_trait]
impl<T> Channel<T> for BinaryOpChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    async fn write(&self, value: T) -> GraphResult<()> {
        let new_value = {
            let mut val = self.value.write().await;
            match val.as_ref() {
                Some(existing) => {
                    let combined = (self.reducer)(existing.clone(), value);
                    *val = Some(combined.clone());
                    combined
                }
                None => {
                    *val = Some(value.clone());
                    value
                }
            }
        };
        {
            let mut ver = self.version.write().await;
            *ver += 1;
        }
        // Ignore send errors (no active receivers)
        let _ = self.sender.send(new_value);
        Ok(())
    }

    async fn read(&self) -> GraphResult<Option<T>> {
        let value = self.value.read().await;
        Ok(value.clone())
    }

    async fn version(&self) -> GraphResult<u64> {
        let version = self.version.read().await;
        Ok(*version)
    }

    async fn clear(&self) -> GraphResult<()> {
        {
            let mut val = self.value.write().await;
            *val = None;
        }
        {
            let mut ver = self.version.write().await;
            *ver += 1;
        }
        Ok(())
    }

    async fn is_updated(&self, since_version: u64) -> GraphResult<bool> {
        let current_version = self.version().await?;
        Ok(current_version > since_version)
    }
}

/// Ephemeral channel that doesn't persist values (for inputs/outputs)
#[derive(Debug)]
pub struct EphemeralChannel<T> {
    version: Arc<RwLock<u64>>,
    sender: broadcast::Sender<T>,
}

impl<T> EphemeralChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    /// Create a new ephemeral channel
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self {
            version: Arc::new(RwLock::new(0)),
            sender,
        }
    }

    /// Subscribe to value updates
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.sender.subscribe()
    }
}

impl<T> Default for EphemeralChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<T> Channel<T> for EphemeralChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    async fn write(&self, value: T) -> GraphResult<()> {
        {
            let mut ver = self.version.write().await;
            *ver += 1;
        }
        // Ignore send errors (no active receivers)
        let _ = self.sender.send(value);
        Ok(())
    }

    async fn read(&self) -> GraphResult<Option<T>> {
        // Ephemeral channels don't store values
        Ok(None)
    }

    async fn version(&self) -> GraphResult<u64> {
        let version = self.version.read().await;
        Ok(*version)
    }

    async fn clear(&self) -> GraphResult<()> {
        {
            let mut ver = self.version.write().await;
            *ver += 1;
        }
        Ok(())
    }

    async fn is_updated(&self, since_version: u64) -> GraphResult<bool> {
        let current_version = self.version().await?;
        Ok(current_version > since_version)
    }
}

/// Channel manager for organizing and accessing channels
pub struct ChannelManager {
    channels: Arc<RwLock<HashMap<String, Arc<dyn ChannelTrait>>>>,
}

pub trait ChannelTrait: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T> ChannelTrait for LastValueChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl<T> ChannelTrait for AccumulatorChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl<T> ChannelTrait for BinaryOpChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl<T> ChannelTrait for EphemeralChannel<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ChannelManager {
    /// Create a new channel manager
    pub fn new() -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a channel
    pub async fn register_channel<T>(
        &self,
        name: impl Into<String>,
        channel: impl ChannelTrait + 'static,
    ) -> GraphResult<()> {
        let mut channels = self.channels.write().await;
        channels.insert(name.into(), Arc::new(channel));
        Ok(())
    }

    /// Get a channel by name
    pub async fn get_channel<T>(&self, name: &str) -> GraphResult<Option<Arc<dyn Channel<T>>>>
    where
        T: Clone + Send + Sync + 'static,
    {
        let channels = self.channels.read().await;
        if let Some(channel) = channels.get(name) {
            if let Some(typed_channel) = channel.as_any().downcast_ref::<LastValueChannel<T>>() {
                return Ok(Some(
                    Arc::new((*typed_channel).clone()) as Arc<dyn Channel<T>>
                ));
            }
            // Add more channel type checks as needed
        }
        Ok(None)
    }

    /// List all channel names
    pub async fn list_channels(&self) -> Vec<String> {
        let channels = self.channels.read().await;
        channels.keys().cloned().collect()
    }

    /// Remove a channel
    pub async fn remove_channel(&self, name: &str) -> GraphResult<bool> {
        let mut channels = self.channels.write().await;
        Ok(channels.remove(name).is_some())
    }

    /// Clear all channels
    pub async fn clear_all(&self) -> GraphResult<()> {
        let mut channels = self.channels.write().await;
        channels.clear();
        Ok(())
    }
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Channel specification for defining channel types and behaviors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSpec {
    /// Channel name
    pub name: String,
    /// Channel type
    pub channel_type: ChannelType,
    /// Whether the channel persists values
    pub persistent: bool,
    /// Initial value (if any)
    pub initial_value: Option<serde_json::Value>,
}

/// Types of channels available
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelType {
    /// Last value channel
    LastValue,
    /// Accumulator channel
    Accumulator,
    /// Binary operator channel with reducer
    BinaryOp { reducer: String },
    /// Ephemeral channel
    Ephemeral,
}

impl ChannelSpec {
    /// Create a new channel specification
    pub fn new(name: impl Into<String>, channel_type: ChannelType) -> Self {
        Self {
            name: name.into(),
            channel_type,
            persistent: true,
            initial_value: None,
        }
    }

    /// Set persistence
    pub fn with_persistence(mut self, persistent: bool) -> Self {
        self.persistent = persistent;
        self
    }

    /// Set initial value
    pub fn with_initial_value<T>(mut self, value: T) -> GraphResult<Self>
    where
        T: Serialize,
    {
        self.initial_value = Some(serde_json::to_value(value)?);
        Ok(self)
    }
}
