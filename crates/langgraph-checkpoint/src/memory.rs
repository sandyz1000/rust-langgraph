//! In-memory checkpointer implementation

use crate::base::{
    Checkpoint, CheckpointConfig, CheckpointMetadata, CheckpointTuple, Checkpointer,
    CheckpointerStats, CleanupResult, PendingWrite,
};
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use langgraph_core::{GraphResult, GraphState, LangGraphError};
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory checkpointer for development and testing
#[derive(Debug)]
pub struct InMemoryCheckpointer {
    /// Storage for checkpoints by thread ID and checkpoint ID
    storage: Arc<DashMap<String, Arc<DashMap<String, CheckpointData>>>>,
    /// Configuration
    config: InMemoryConfig,
    /// Statistics
    stats: Arc<RwLock<CheckpointerStats>>,
}

/// Stored checkpoint data
#[derive(Debug, Clone)]
struct CheckpointData {
    /// Serialized checkpoint
    checkpoint_json: String,
    /// Checkpoint metadata
    metadata: CheckpointMetadata,
    /// Pending writes
    pending_writes: Vec<PendingWrite>,
    /// Storage timestamp
    stored_at: chrono::DateTime<chrono::Utc>,
    /// Size in bytes
    size_bytes: usize,
}

/// Configuration for in-memory checkpointer
#[derive(Debug, Clone)]
pub struct InMemoryConfig {
    /// Maximum checkpoints per thread
    pub max_checkpoints_per_thread: usize,
    /// Maximum total checkpoints
    pub max_total_checkpoints: usize,
    /// Enable automatic cleanup
    pub auto_cleanup: bool,
    /// Cleanup interval in seconds
    pub cleanup_interval_seconds: u64,
}

impl Default for InMemoryConfig {
    fn default() -> Self {
        Self {
            max_checkpoints_per_thread: 100,
            max_total_checkpoints: 10000,
            auto_cleanup: true,
            cleanup_interval_seconds: 300, // 5 minutes
        }
    }
}

impl InMemoryCheckpointer {
    /// Create a new in-memory checkpointer
    pub fn new() -> Self {
        Self::with_config(InMemoryConfig::default())
    }

    /// Create in-memory checkpointer with configuration
    pub fn with_config(config: InMemoryConfig) -> Self {
        let checkpointer = Self {
            storage: Arc::new(DashMap::new()),
            config,
            stats: Arc::new(RwLock::new(CheckpointerStats {
                total_checkpoints: 0,
                unique_threads: 0,
                storage_size_bytes: 0,
                avg_checkpoint_size_bytes: 0,
                oldest_checkpoint: None,
                newest_checkpoint: None,
            })),
        };

        // Start cleanup task if auto cleanup is enabled
        if checkpointer.config.auto_cleanup {
            let checkpointer_clone = checkpointer.clone();
            tokio::spawn(async move {
                checkpointer_clone.cleanup_task().await;
            });
        }

        checkpointer
    }

    /// Cleanup task that runs periodically
    async fn cleanup_task(&self) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.cleanup_interval_seconds,
        ));

        loop {
            interval.tick().await;
            if let Err(e) = Checkpointer::<()>::cleanup(self).await {
                tracing::warn!("Cleanup task failed: {}", e);
            }
        }
    }

    /// Get thread storage
    fn get_thread_storage(&self, thread_id: &str) -> Arc<DashMap<String, CheckpointData>> {
        if let Some(storage) = self.storage.get(thread_id) {
            storage.value().clone()
        } else {
            let new_storage = Arc::new(DashMap::new());
            self.storage
                .insert(thread_id.to_string(), new_storage.clone());
            new_storage
        }
    }

    /// Enforce checkpoint limits
    async fn enforce_limits(&self, thread_id: &str) -> GraphResult<()> {
        let thread_storage = self.get_thread_storage(thread_id);

        // Remove oldest checkpoints if over limit
        if thread_storage.len() > self.config.max_checkpoints_per_thread {
            let excess = thread_storage.len() - self.config.max_checkpoints_per_thread;
            let mut entries: Vec<_> = thread_storage
                .iter()
                .map(|entry| (entry.key().clone(), entry.value().stored_at))
                .collect();

            // Sort by timestamp (oldest first)
            entries.sort_by_key(|(_, timestamp)| *timestamp);

            // Remove oldest entries
            for i in 0..excess {
                thread_storage.remove(&entries[i].0);
            }
        }

        // Check total checkpoint limit
        let total_checkpoints: usize = self.storage.iter().map(|entry| entry.len()).sum();
        if total_checkpoints > self.config.max_total_checkpoints {
            // Remove oldest checkpoints across all threads
            let excess = total_checkpoints - self.config.max_total_checkpoints;
            let mut all_entries = Vec::new();

            for thread_entry in self.storage.iter() {
                let thread_id = thread_entry.key().clone();
                for checkpoint_entry in thread_entry.value().iter() {
                    all_entries.push((
                        thread_id.clone(),
                        checkpoint_entry.key().clone(),
                        checkpoint_entry.value().stored_at,
                    ));
                }
            }

            // Sort by timestamp (oldest first)
            all_entries.sort_by_key(|(_, _, timestamp)| *timestamp);

            // Remove oldest entries
            for i in 0..excess.min(all_entries.len()) {
                let (thread_id, checkpoint_id, _) = &all_entries[i];
                if let Some(thread_storage) = self.storage.get(thread_id) {
                    thread_storage.remove(checkpoint_id);
                }
            }
        }

        Ok(())
    }

    /// Update statistics
    async fn update_stats(&self) -> GraphResult<()> {
        let mut stats = self.stats.write().await;

        let mut total_checkpoints = 0;
        let mut total_size_bytes = 0;
        let mut oldest_timestamp = None;
        let mut newest_timestamp = None;

        for thread_entry in self.storage.iter() {
            for checkpoint_entry in thread_entry.value().iter() {
                total_checkpoints += 1;
                total_size_bytes += checkpoint_entry.value().size_bytes;

                let timestamp = checkpoint_entry.value().stored_at;
                if oldest_timestamp.is_none() || timestamp < oldest_timestamp.unwrap() {
                    oldest_timestamp = Some(timestamp);
                }
                if newest_timestamp.is_none() || timestamp > newest_timestamp.unwrap() {
                    newest_timestamp = Some(timestamp);
                }
            }
        }

        stats.total_checkpoints = total_checkpoints;
        stats.unique_threads = self.storage.len();
        stats.storage_size_bytes = total_size_bytes as u64;
        stats.avg_checkpoint_size_bytes = if total_checkpoints > 0 {
            total_size_bytes as u64 / total_checkpoints as u64
        } else {
            0
        };
        stats.oldest_checkpoint = oldest_timestamp;
        stats.newest_checkpoint = newest_timestamp;

        Ok(())
    }
}

impl Clone for InMemoryCheckpointer {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            config: self.config.clone(),
            stats: self.stats.clone(),
        }
    }
}

impl Default for InMemoryCheckpointer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: GraphState> Checkpointer<S> for InMemoryCheckpointer {
    async fn put(
        &self,
        config: &CheckpointConfig,
        checkpoint: Checkpoint<S>,
        metadata: CheckpointMetadata,
    ) -> GraphResult<()> {
        // Serialize checkpoint
        let checkpoint_json = serde_json::to_string(&checkpoint)
            .map_err(|e| LangGraphError::Serialization { source: e })?;

        let size_bytes = checkpoint_json.len();

        let checkpoint_data = CheckpointData {
            checkpoint_json,
            metadata,
            pending_writes: Vec::new(),
            stored_at: Utc::now(),
            size_bytes,
        };

        // Store checkpoint
        let thread_storage = self.get_thread_storage(&config.thread_id);
        thread_storage.insert(checkpoint.id.clone(), checkpoint_data);

        // Enforce limits
        self.enforce_limits(&config.thread_id).await?;

        // Update statistics
        self.update_stats().await?;

        Ok(())
    }

    async fn get(
        &self,
        config: &CheckpointConfig,
        checkpoint_id: &str,
    ) -> GraphResult<Option<CheckpointTuple<S>>> {
        let thread_storage = self.get_thread_storage(&config.thread_id);

        if let Some(data) = thread_storage.get(checkpoint_id) {
            // Deserialize the checkpoint from JSON
            let checkpoint: Checkpoint<S> = serde_json::from_str(&data.checkpoint_json)
                .map_err(|e| LangGraphError::Serialization { source: e })?;

            return Ok(Some(CheckpointTuple {
                checkpoint,
                pending_writes: data.pending_writes.clone(),
                config: config.clone(),
            }));
        }

        Ok(None)
    }

    async fn get_latest(
        &self,
        config: &CheckpointConfig,
    ) -> GraphResult<Option<CheckpointTuple<S>>> {
        let thread_storage = self.get_thread_storage(&config.thread_id);

        if thread_storage.is_empty() {
            return Ok(None);
        }

        // Find the latest checkpoint by timestamp
        let mut latest_entry = None;
        let mut latest_timestamp = None;

        for entry in thread_storage.iter() {
            let timestamp = entry.value().stored_at;
            if latest_timestamp.is_none() || timestamp > latest_timestamp.unwrap() {
                latest_timestamp = Some(timestamp);
                latest_entry = Some((entry.key().clone(), entry.value().clone()));
            }
        }

        if let Some((checkpoint_id, data)) = latest_entry {
            // Deserialize the checkpoint data
            let checkpoint: Checkpoint<S> =
                serde_json::from_str(&data.checkpoint_json).map_err(|e| {
                    LangGraphError::runtime(format!("Failed to deserialize checkpoint: {}", e))
                })?;

            let tuple = CheckpointTuple {
                checkpoint,
                pending_writes: data.pending_writes.clone(),
                config: config.clone(),
            };

            return Ok(Some(tuple));
        }

        Ok(None)
    }

    async fn list(
        &self,
        config: &CheckpointConfig,
        limit: Option<usize>,
        before: Option<&str>,
    ) -> GraphResult<Vec<CheckpointTuple<S>>> {
        let thread_storage = self.get_thread_storage(&config.thread_id);

        let mut entries: Vec<_> = thread_storage
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        // Sort by timestamp (newest first)
        entries.sort_by(|(_, a), (_, b)| b.stored_at.cmp(&a.stored_at));

        // Apply before filter
        if let Some(before_id) = before {
            if let Some(before_data) = thread_storage.get(before_id) {
                let before_timestamp = before_data.stored_at;
                entries.retain(|(_, data)| data.stored_at < before_timestamp);
            }
        }

        // Apply limit
        if let Some(limit) = limit {
            entries.truncate(limit);
        }

        // Convert to CheckpointTuple objects
        let mut result = Vec::new();
        for (checkpoint_id, data) in entries {
            let checkpoint: Checkpoint<S> =
                serde_json::from_str(&data.checkpoint_json).map_err(|e| {
                    LangGraphError::runtime(format!("Failed to deserialize checkpoint: {}", e))
                })?;

            let tuple = CheckpointTuple {
                checkpoint,
                pending_writes: data.pending_writes.clone(),
                config: config.clone(),
            };

            result.push(tuple);
        }

        Ok(result)
    }

    async fn delete(&self, config: &CheckpointConfig, checkpoint_id: &str) -> GraphResult<bool> {
        let thread_storage = self.get_thread_storage(&config.thread_id);
        let removed = thread_storage.remove(checkpoint_id).is_some();

        if removed {
            self.update_stats().await?;
        }

        Ok(removed)
    }

    async fn clear_thread(&self, thread_id: &str) -> GraphResult<usize> {
        if let Some((_, thread_storage)) = self.storage.remove(thread_id) {
            let count = thread_storage.len();
            self.update_stats().await?;
            Ok(count)
        } else {
            Ok(0)
        }
    }

    async fn stats(&self) -> GraphResult<CheckpointerStats> {
        self.update_stats().await?;
        let stats = self.stats.read().await;
        Ok(stats.clone())
    }

    async fn cleanup(&self) -> GraphResult<CleanupResult> {
        let start_time = std::time::Instant::now();
        let checkpoints_removed = 0;
        let space_freed_bytes = 0;

        // Remove empty thread storages
        let thread_ids: Vec<String> = self
            .storage
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        for thread_id in thread_ids {
            if let Some(thread_storage) = self.storage.get(&thread_id) {
                if thread_storage.is_empty() {
                    self.storage.remove(&thread_id);
                }
            }
        }

        // Update statistics after cleanup
        self.update_stats().await?;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(CleanupResult {
            checkpoints_removed,
            space_freed_bytes,
            duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestState {
        value: i32,
    }

    #[tokio::test]
    async fn test_basic_operations() {
        let checkpointer = InMemoryCheckpointer::new();
        let config = CheckpointConfig::default();

        let checkpoint = Checkpoint::new("test-checkpoint".to_string(), TestState { value: 42 }, 1);
        let metadata = CheckpointMetadata::new();

        // Put checkpoint
        Checkpointer::<TestState>::put(&checkpointer, &config, checkpoint.clone(), metadata)
            .await
            .unwrap();

        // Get checkpoint
        let retrieved = Checkpointer::<TestState>::get(&checkpointer, &config, "test-checkpoint")
            .await
            .unwrap();
        assert!(retrieved.is_some());

        let checkpoint_tuple = retrieved.unwrap();
        assert_eq!(checkpoint_tuple.checkpoint.id, "test-checkpoint");
        assert_eq!(checkpoint_tuple.checkpoint.state.value, 42);

        // Get latest
        let latest = Checkpointer::<TestState>::get_latest(&checkpointer, &config)
            .await
            .unwrap();
        assert!(latest.is_some());

        // Delete checkpoint
        let deleted = Checkpointer::<TestState>::delete(&checkpointer, &config, "test-checkpoint")
            .await
            .unwrap();
        assert!(deleted);

        // Verify deletion
        let retrieved_after_delete =
            Checkpointer::<TestState>::get(&checkpointer, &config, "test-checkpoint")
                .await
                .unwrap();
        assert!(retrieved_after_delete.is_none());
    }

    #[tokio::test]
    async fn test_list_checkpoints() {
        let checkpointer = InMemoryCheckpointer::new();
        let config = CheckpointConfig::default();

        // Create multiple checkpoints
        for i in 0..5 {
            let checkpoint = Checkpoint::new(
                format!("checkpoint-{}", i),
                TestState { value: i },
                i as u32,
            );
            let metadata = CheckpointMetadata::new();
            Checkpointer::<TestState>::put(&checkpointer, &config, checkpoint, metadata)
                .await
                .unwrap();

            // Small delay to ensure different timestamps
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        // List all checkpoints
        let all_checkpoints = Checkpointer::<TestState>::list(&checkpointer, &config, None, None)
            .await
            .unwrap();
        assert_eq!(all_checkpoints.len(), 5);

        // List with limit
        let limited_checkpoints =
            Checkpointer::<TestState>::list(&checkpointer, &config, Some(3), None)
                .await
                .unwrap();
        assert_eq!(limited_checkpoints.len(), 3);

        // Verify ordering (newest first)
        assert!(
            limited_checkpoints[0].checkpoint.timestamp
                >= limited_checkpoints[1].checkpoint.timestamp
        );
        assert!(
            limited_checkpoints[1].checkpoint.timestamp
                >= limited_checkpoints[2].checkpoint.timestamp
        );
    }

    #[tokio::test]
    async fn test_stats() {
        let checkpointer = InMemoryCheckpointer::new();
        let config = CheckpointConfig::default();

        // Initially empty
        let initial_stats = Checkpointer::<TestState>::stats(&checkpointer)
            .await
            .unwrap();
        assert_eq!(initial_stats.total_checkpoints, 0);
        assert_eq!(initial_stats.unique_threads, 0);

        // Add some checkpoints
        for i in 0..3 {
            let checkpoint = Checkpoint::new(
                format!("checkpoint-{}", i),
                TestState { value: i },
                i as u32,
            );
            let metadata = CheckpointMetadata::new();
            Checkpointer::<TestState>::put(&checkpointer, &config, checkpoint, metadata)
                .await
                .unwrap();
        }

        let stats = Checkpointer::<TestState>::stats(&checkpointer)
            .await
            .unwrap();
        assert_eq!(stats.total_checkpoints, 3);
        assert_eq!(stats.unique_threads, 1);
        assert!(stats.storage_size_bytes > 0);
    }
}
