//! Redis checkpointer implementation using fred.rs

use crate::base::{
    Checkpoint, CheckpointConfig, CheckpointMetadata, CheckpointTuple, Checkpointer,
    CheckpointerStats, CleanupResult, PendingWrite,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fred::{
    clients::RedisClient,
    interfaces::*,
    types::{RedisConfig, RedisValue},
};
use langgraph_core::{GraphResult, GraphState, LangGraphError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// Helper function to create a checkpoint error
fn checkpoint_error(message: impl Into<String>) -> LangGraphError {
    LangGraphError::Checkpoint {
        source: message.into().into(),
    }
}

/// Redis checkpointer for scalable persistent storage
#[derive(Debug, Clone)]
pub struct RedisCheckpointer {
    client: RedisClient,
    config: RedisCheckpointerConfig,
}

/// Configuration for Redis checkpointer
#[derive(Debug, Clone)]
pub struct RedisCheckpointerConfig {
    /// Redis connection URL
    pub redis_url: String,
    /// Key prefix for checkpoints
    pub key_prefix: String,
    /// Enable compression for checkpoint data
    pub compress: bool,
    /// Default TTL for checkpoints in seconds (None for no expiration)
    pub default_ttl_seconds: Option<u64>,
    /// Maximum number of checkpoints to keep per thread
    pub max_checkpoints_per_thread: Option<usize>,
    /// Connection pool size
    pub pool_size: Option<usize>,
    /// Connection timeout in milliseconds
    pub connect_timeout_ms: Option<u64>,
    /// Command timeout in milliseconds
    pub command_timeout_ms: Option<u64>,
}

impl Default for RedisCheckpointerConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://127.0.0.1:6379".to_string(),
            key_prefix: "langgraph:checkpoint".to_string(),
            compress: true,
            default_ttl_seconds: Some(24 * 60 * 60), // 24 hours
            max_checkpoints_per_thread: Some(100),
            pool_size: Some(10),
            connect_timeout_ms: Some(5000),
            command_timeout_ms: Some(5000),
        }
    }
}

/// Serialized checkpoint data for Redis storage
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RedisCheckpointData {
    /// Compressed/serialized checkpoint
    checkpoint_data: Vec<u8>,
    /// Pending writes
    pending_writes: Vec<PendingWrite>,
    /// Storage timestamp
    stored_at: DateTime<Utc>,
    /// Size in bytes (before compression)
    original_size_bytes: usize,
    /// Metadata for indexing
    metadata: CheckpointMetadata,
}

impl RedisCheckpointer {
    /// Create a new Redis checkpointer with default configuration
    pub async fn new(redis_url: impl Into<String>) -> GraphResult<Self> {
        let config = RedisCheckpointerConfig {
            redis_url: redis_url.into(),
            ..Default::default()
        };
        Self::with_config(config).await
    }

    /// Create Redis checkpointer with custom configuration
    pub async fn with_config(config: RedisCheckpointerConfig) -> GraphResult<Self> {
        // Parse Redis configuration
        let redis_config = RedisConfig::from_url(&config.redis_url)
            .map_err(|e| checkpoint_error(format!("Invalid Redis URL: {}", e)))?;

        // Create Redis client
        let client = RedisClient::new(redis_config, None, None, None);

        // Connect to Redis
        client.connect();
        client
            .wait_for_connect()
            .await
            .map_err(|e| checkpoint_error(format!("Failed to connect to Redis: {}", e)))?;

        info!("Connected to Redis at {}", config.redis_url);

        Ok(Self { client, config })
    }

    /// Generate Redis key for a checkpoint
    fn checkpoint_key(&self, thread_id: &str, checkpoint_id: &str) -> String {
        format!("{}:{}:{}", self.config.key_prefix, thread_id, checkpoint_id)
    }

    /// Generate Redis key for thread checkpoint list
    fn thread_list_key(&self, thread_id: &str) -> String {
        format!("{}:thread:{}:list", self.config.key_prefix, thread_id)
    }

    /// Generate Redis key for thread metadata
    fn thread_metadata_key(&self, thread_id: &str) -> String {
        format!("{}:thread:{}:meta", self.config.key_prefix, thread_id)
    }

    /// Generate Redis key for global statistics
    fn stats_key(&self) -> String {
        format!("{}:stats", self.config.key_prefix)
    }

    /// Compress data if compression is enabled
    fn compress_data(&self, data: &[u8]) -> GraphResult<Vec<u8>> {
        if self.config.compress {
            use flate2::{write::GzEncoder, Compression};
            use std::io::Write;

            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder
                .write_all(data)
                .map_err(|e| checkpoint_error(format!("Compression failed: {}", e)))?;
            encoder
                .finish()
                .map_err(|e| checkpoint_error(format!("Compression finalization failed: {}", e)))
        } else {
            Ok(data.to_vec())
        }
    }

    /// Decompress data if compression is enabled
    fn decompress_data(&self, data: &[u8]) -> GraphResult<Vec<u8>> {
        if self.config.compress {
            use flate2::read::GzDecoder;
            use std::io::Read;

            let mut decoder = GzDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| checkpoint_error(format!("Decompression failed: {}", e)))?;
            Ok(decompressed)
        } else {
            Ok(data.to_vec())
        }
    }

    /// Serialize and compress checkpoint data
    async fn serialize_checkpoint<S: GraphState>(
        &self,
        checkpoint: &Checkpoint<S>,
        pending_writes: &[PendingWrite],
        metadata: &CheckpointMetadata,
    ) -> GraphResult<Vec<u8>> {
        // Serialize checkpoint using MessagePack for efficiency
        let checkpoint_data = rmp_serde::to_vec(checkpoint)
            .map_err(|e| checkpoint_error(format!("Failed to serialize checkpoint: {}", e)))?;

        let original_size = checkpoint_data.len();
        let compressed_data = self.compress_data(&checkpoint_data)?;

        let redis_data = RedisCheckpointData {
            checkpoint_data: compressed_data,
            pending_writes: pending_writes.to_vec(),
            stored_at: Utc::now(),
            original_size_bytes: original_size,
            metadata: metadata.clone(),
        };

        serde_json::to_vec(&redis_data)
            .map_err(|e| checkpoint_error(format!("Failed to serialize Redis data: {}", e)))
    }

    /// Deserialize and decompress checkpoint data
    async fn deserialize_checkpoint<S: GraphState>(
        &self,
        data: &[u8],
    ) -> GraphResult<(Checkpoint<S>, Vec<PendingWrite>, CheckpointMetadata)> {
        // Deserialize Redis data structure
        let redis_data: RedisCheckpointData = serde_json::from_slice(data)
            .map_err(|e| checkpoint_error(format!("Failed to deserialize Redis data: {}", e)))?;

        // Decompress checkpoint data
        let decompressed_data = self.decompress_data(&redis_data.checkpoint_data)?;

        // Deserialize checkpoint
        let checkpoint: Checkpoint<S> = rmp_serde::from_slice(&decompressed_data)
            .map_err(|e| checkpoint_error(format!("Failed to deserialize checkpoint: {}", e)))?;

        Ok((checkpoint, redis_data.pending_writes, redis_data.metadata))
    }

    /// Update thread checkpoint list
    async fn update_thread_list(&self, thread_id: &str, checkpoint_id: &str) -> GraphResult<()> {
        let list_key = self.thread_list_key(thread_id);

        // Add checkpoint to the front of the list
        self.client
            .lpush(&list_key, checkpoint_id)
            .await
            .map_err(|e| checkpoint_error(format!("Failed to update thread list: {}", e)))?;

        // Trim list to max size if configured
        if let Some(max_checkpoints) = self.config.max_checkpoints_per_thread {
            self.client
                .ltrim(&list_key, 0, max_checkpoints as i64 - 1)
                .await
                .map_err(|e| checkpoint_error(format!("Failed to trim thread list: {}", e)))?;
        }

        // Set TTL if configured
        if let Some(ttl) = self.config.default_ttl_seconds {
            self.client
                .expire(&list_key, ttl as i64)
                .await
                .map_err(|e| {
                    checkpoint_error(format!("Failed to set TTL on thread list: {}", e))
                })?;
        }

        Ok(())
    }

    /// Get checkpoint IDs for a thread
    async fn get_thread_checkpoint_ids(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> GraphResult<Vec<String>> {
        let list_key = self.thread_list_key(thread_id);
        let end = limit.map(|l| l as i64 - 1).unwrap_or(-1);

        let ids: Vec<String> =
            self.client.lrange(&list_key, 0, end).await.map_err(|e| {
                checkpoint_error(format!("Failed to get thread checkpoint IDs: {}", e))
            })?;

        Ok(ids)
    }

    /// Update global statistics
    async fn update_stats(&self, size_delta: i64, count_delta: i64) -> GraphResult<()> {
        let stats_key = self.stats_key();

        if count_delta != 0 {
            let _: RedisValue = self
                .client
                .hincrby(&stats_key, "total_checkpoints", count_delta)
                .await
                .map_err(|e| {
                    checkpoint_error(format!("Failed to update checkpoint count: {}", e))
                })?;
        }

        if size_delta != 0 {
            let _: RedisValue = self
                .client
                .hincrby(&stats_key, "total_size_bytes", size_delta)
                .await
                .map_err(|e| checkpoint_error(format!("Failed to update total size: {}", e)))?;
        }

        // Update timestamp
        let now = Utc::now().timestamp();
        let _: RedisValue = self
            .client
            .hset(&stats_key, ("last_updated", now))
            .await
            .map_err(|e| checkpoint_error(format!("Failed to update stats timestamp: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl<S: GraphState> Checkpointer<S> for RedisCheckpointer {
    async fn put(
        &self,
        config: &CheckpointConfig,
        checkpoint: Checkpoint<S>,
        metadata: CheckpointMetadata,
    ) -> GraphResult<()> {
        debug!(
            "Storing checkpoint {} for thread {}",
            checkpoint.id, config.thread_id
        );

        // Serialize checkpoint data
        let serialized_data = self
            .serialize_checkpoint(&checkpoint, &[], &metadata)
            .await?;
        let key = self.checkpoint_key(&config.thread_id, &checkpoint.id);

        // Store checkpoint in Redis
        self.client
            .set(&key, &serialized_data)
            .await
            .map_err(|e| checkpoint_error(format!("Failed to store checkpoint: {}", e)))?;

        // Set TTL if configured
        if let Some(ttl) = self.config.default_ttl_seconds {
            self.client
                .expire(&key, ttl as i64)
                .await
                .map_err(|e| checkpoint_error(format!("Failed to set TTL: {}", e)))?;
        }

        // Update thread checkpoint list
        self.update_thread_list(&config.thread_id, &checkpoint.id)
            .await?;

        // Update statistics
        self.update_stats(serialized_data.len() as i64, 1).await?;

        info!(
            "Stored checkpoint {} for thread {}",
            checkpoint.id, config.thread_id
        );
        Ok(())
    }

    async fn get(
        &self,
        config: &CheckpointConfig,
        checkpoint_id: &str,
    ) -> GraphResult<Option<CheckpointTuple<S>>> {
        debug!(
            "Retrieving checkpoint {} for thread {}",
            checkpoint_id, config.thread_id
        );

        let key = self.checkpoint_key(&config.thread_id, checkpoint_id);

        let data: Option<RedisValue> = self
            .client
            .get(&key)
            .await
            .map_err(|e| checkpoint_error(format!("Failed to retrieve checkpoint: {}", e)))?;

        match data {
            Some(redis_value) => {
                let data: Vec<u8> = redis_value.into_bytes().map_err(|e| {
                    checkpoint_error(format!("Failed to convert Redis value to bytes: {}", e))
                })?;
                let (checkpoint, pending_writes, metadata) =
                    self.deserialize_checkpoint(&data).await?;

                Ok(Some(CheckpointTuple {
                    checkpoint,
                    pending_writes,
                    config: config.clone(),
                }))
            }
            None => {
                debug!(
                    "Checkpoint {} not found for thread {}",
                    checkpoint_id, config.thread_id
                );
                Ok(None)
            }
        }
    }

    async fn get_latest(
        &self,
        config: &CheckpointConfig,
    ) -> GraphResult<Option<CheckpointTuple<S>>> {
        debug!(
            "Retrieving latest checkpoint for thread {}",
            config.thread_id
        );

        // Get the latest checkpoint ID from the thread list
        let checkpoint_ids = self
            .get_thread_checkpoint_ids(&config.thread_id, Some(1))
            .await?;

        match checkpoint_ids.first() {
            Some(checkpoint_id) => self.get(config, checkpoint_id).await,
            None => {
                debug!("No checkpoints found for thread {}", config.thread_id);
                Ok(None)
            }
        }
    }

    async fn list(
        &self,
        config: &CheckpointConfig,
        limit: Option<usize>,
        before: Option<&str>,
    ) -> GraphResult<Vec<CheckpointTuple<S>>> {
        debug!(
            "Listing checkpoints for thread {} (limit: {:?}, before: {:?})",
            config.thread_id, limit, before
        );

        let mut checkpoint_ids = self
            .get_thread_checkpoint_ids(&config.thread_id, None)
            .await?;

        // Filter by 'before' parameter if provided
        if let Some(before_id) = before {
            if let Some(pos) = checkpoint_ids.iter().position(|id| id == before_id) {
                checkpoint_ids = checkpoint_ids[pos + 1..].to_vec();
            }
        }

        // Apply limit
        if let Some(limit) = limit {
            checkpoint_ids.truncate(limit);
        }

        // Fetch all checkpoints in parallel
        let mut checkpoints = Vec::new();
        for checkpoint_id in checkpoint_ids {
            if let Some(tuple) = self.get(config, &checkpoint_id).await? {
                checkpoints.push(tuple);
            }
        }

        Ok(checkpoints)
    }

    async fn delete(&self, config: &CheckpointConfig, checkpoint_id: &str) -> GraphResult<bool> {
        debug!(
            "Deleting checkpoint {} for thread {}",
            checkpoint_id, config.thread_id
        );

        let key = self.checkpoint_key(&config.thread_id, checkpoint_id);

        // Delete checkpoint
        let deleted: i64 = self
            .client
            .del(&key)
            .await
            .map_err(|e| checkpoint_error(format!("Failed to delete checkpoint: {}", e)))?;

        if deleted > 0 {
            // Remove from thread list
            let list_key = self.thread_list_key(&config.thread_id);
            self.client
                .lrem(&list_key, 0, checkpoint_id)
                .await
                .map_err(|e| {
                    checkpoint_error(format!("Failed to remove from thread list: {}", e))
                })?;

            // Update statistics
            self.update_stats(0, -1).await?;

            info!(
                "Deleted checkpoint {} for thread {}",
                checkpoint_id, config.thread_id
            );
            Ok(true)
        } else {
            debug!("Checkpoint {} not found for deletion", checkpoint_id);
            Ok(false)
        }
    }

    async fn clear_thread(&self, thread_id: &str) -> GraphResult<usize> {
        info!("Clearing all checkpoints for thread {}", thread_id);

        // Get all checkpoint IDs for the thread
        let checkpoint_ids = self.get_thread_checkpoint_ids(thread_id, None).await?;
        let mut deleted_count = 0;

        // Delete each checkpoint
        for checkpoint_id in &checkpoint_ids {
            let key = self.checkpoint_key(thread_id, checkpoint_id);
            let deleted: i64 = self.client.del(&key).await.map_err(|e| {
                checkpoint_error(format!(
                    "Failed to delete checkpoint {}: {}",
                    checkpoint_id, e
                ))
            })?;

            if deleted > 0 {
                deleted_count += 1;
            }
        }

        // Clear the thread list
        let list_key = self.thread_list_key(thread_id);
        self.client
            .del(&list_key)
            .await
            .map_err(|e| checkpoint_error(format!("Failed to delete thread list: {}", e)))?;

        // Clear thread metadata
        let meta_key = self.thread_metadata_key(thread_id);
        self.client
            .del(&meta_key)
            .await
            .map_err(|e| checkpoint_error(format!("Failed to delete thread metadata: {}", e)))?;

        // Update statistics
        self.update_stats(0, -(deleted_count as i64)).await?;

        info!(
            "Cleared {} checkpoints for thread {}",
            deleted_count, thread_id
        );
        Ok(deleted_count)
    }

    async fn stats(&self) -> GraphResult<CheckpointerStats> {
        debug!("Retrieving checkpointer statistics");

        let stats_key = self.stats_key();
        let stats: HashMap<String, RedisValue> = self
            .client
            .hgetall(&stats_key)
            .await
            .map_err(|e| checkpoint_error(format!("Failed to get statistics: {}", e)))?;

        let total_checkpoints = stats
            .get("total_checkpoints")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as usize;

        let total_size = stats
            .get("total_size_bytes")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;

        let avg_size = if total_checkpoints > 0 {
            total_size / total_checkpoints as u64
        } else {
            0
        };

        Ok(CheckpointerStats {
            total_checkpoints,
            unique_threads: 0, // Would require more complex scanning
            storage_size_bytes: total_size,
            avg_checkpoint_size_bytes: avg_size,
            oldest_checkpoint: None, // Would require scanning all checkpoints
            newest_checkpoint: None, // Would require scanning all checkpoints
        })
    }

    async fn cleanup(&self) -> GraphResult<CleanupResult> {
        let start_time = std::time::Instant::now();
        let checkpoints_removed = 0;
        let space_freed_bytes = 0;

        info!("Starting Redis checkpoint cleanup");

        // Since Redis handles TTL automatically, we mainly just clean up empty lists
        // In a full implementation, you could scan for patterns and clean up here

        let duration_ms = start_time.elapsed().as_millis() as u64;

        info!(
            "Cleanup completed: removed {} checkpoints, freed {} bytes in {}ms",
            checkpoints_removed, space_freed_bytes, duration_ms
        );

        Ok(CleanupResult {
            checkpoints_removed,
            space_freed_bytes,
            duration_ms,
        })
    }
}

impl Default for RedisCheckpointer {
    fn default() -> Self {
        // This is a synchronous default, so we can't actually connect
        // Users should use new() or with_config() instead
        panic!(
            "Use RedisCheckpointer::new() or RedisCheckpointer::with_config() to create instances"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    // Note: These tests require a running Redis instance
    #[tokio::test]
    #[ignore] // Ignore by default as it requires Redis
    async fn test_redis_checkpointer_basic() {
        // This would require a running Redis instance for testing
        // In a real implementation, you'd set up integration tests
        // with a test Redis container
    }
}
