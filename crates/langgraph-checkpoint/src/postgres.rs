//! PostgreSQL checkpointer implementation

use crate::base::{
    Checkpoint, CheckpointConfig, CheckpointMetadata, CheckpointTuple, Checkpointer,
    CheckpointerStats, CleanupResult, PendingWrite,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use langgraph_core::{GraphResult, GraphState, LangGraphError};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tracing::{debug, error, info, warn};

/// Helper function to create a checkpoint error
fn checkpoint_error(message: impl Into<String>) -> LangGraphError {
    LangGraphError::Checkpoint {
        source: message.into().into(),
    }
}

/// PostgreSQL checkpointer for production-grade persistent storage
#[derive(Debug, Clone)]
pub struct PostgresCheckpointer {
    pool: PgPool,
    config: PostgresConfig,
}

/// Configuration for PostgreSQL checkpointer
#[derive(Debug, Clone)]
pub struct PostgresConfig {
    /// Database connection string
    pub database_url: String,
    /// Maximum number of connections in the pool
    pub max_connections: u32,
    /// Connection timeout in seconds
    pub connect_timeout_seconds: u64,
    /// Enable compression for checkpoint data
    pub compress: bool,
    /// Table prefix for checkpoints
    pub table_prefix: String,
    /// Cleanup threshold in days
    pub cleanup_days: u32,
    /// Enable connection pooling
    pub enable_pooling: bool,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            database_url: "postgresql://localhost/checkpoints".to_string(),
            max_connections: 20,
            connect_timeout_seconds: 30,
            compress: true,
            table_prefix: "langgraph".to_string(),
            cleanup_days: 30,
            enable_pooling: true,
        }
    }
}

impl PostgresCheckpointer {
    /// Create a new PostgreSQL checkpointer
    pub async fn new(database_url: impl Into<String>) -> GraphResult<Self> {
        let config = PostgresConfig {
            database_url: database_url.into(),
            ..Default::default()
        };
        Self::with_config(config).await
    }

    /// Create PostgreSQL checkpointer with configuration
    pub async fn with_config(config: PostgresConfig) -> GraphResult<Self> {
        // Create connection pool
        let pool = PgPool::connect(&config.database_url).await.map_err(|e| {
            checkpoint_error(format!("Failed to connect to PostgreSQL database: {}", e))
        })?;

        let checkpointer = Self { pool, config };

        // Initialize database schema
        checkpointer.init_schema().await?;

        Ok(checkpointer)
    }

    /// Get the full table name with prefix
    fn table_name(&self) -> String {
        format!("{}_checkpoints", self.config.table_prefix)
    }

    /// Initialize database schema
    async fn init_schema(&self) -> GraphResult<()> {
        let table_name = self.table_name();
        let schema = format!(
            r#"
        CREATE TABLE IF NOT EXISTS {} (
            id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL,
            checkpoint_data BYTEA NOT NULL,
            metadata JSONB NOT NULL,
            pending_writes JSONB NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            step_number INTEGER NOT NULL,
            size_bytes BIGINT NOT NULL,
            compressed BOOLEAN NOT NULL DEFAULT FALSE
        );

        CREATE INDEX IF NOT EXISTS idx_{}_thread_id ON {}(thread_id);
        CREATE INDEX IF NOT EXISTS idx_{}_created_at ON {}(created_at);
        CREATE INDEX IF NOT EXISTS idx_{}_step ON {}(step_number);
        CREATE INDEX IF NOT EXISTS idx_{}_metadata ON {} USING GIN (metadata);
        "#,
            table_name,
            self.config.table_prefix,
            table_name,
            self.config.table_prefix,
            table_name,
            self.config.table_prefix,
            table_name,
            self.config.table_prefix,
            table_name
        );

        sqlx::query(&schema)
            .execute(&self.pool)
            .await
            .map_err(|e| checkpoint_error(format!("Failed to initialize schema: {}", e)))?;

        info!("PostgreSQL checkpointer initialized successfully");
        Ok(())
    }

    /// Serialize and optionally compress checkpoint data
    fn serialize_checkpoint<S: GraphState>(
        &self,
        checkpoint: &Checkpoint<S>,
    ) -> GraphResult<Vec<u8>> {
        // Use MessagePack for better performance than JSON
        let msgpack_data = rmp_serde::to_vec(checkpoint)
            .map_err(|e| checkpoint_error(format!("Failed to serialize checkpoint: {}", e)))?;

        if self.config.compress {
            use flate2::write::GzEncoder;
            use flate2::Compression;
            use std::io::Write;

            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder
                .write_all(&msgpack_data)
                .map_err(|e| checkpoint_error(format!("Failed to compress data: {}", e)))?;
            encoder
                .finish()
                .map_err(|e| checkpoint_error(format!("Failed to finalize compression: {}", e)))
        } else {
            Ok(msgpack_data)
        }
    }

    /// Deserialize and optionally decompress checkpoint data
    fn deserialize_checkpoint<S: GraphState>(
        &self,
        data: &[u8],
        compressed: bool,
    ) -> GraphResult<Checkpoint<S>> {
        let msgpack_data = if compressed {
            use flate2::read::GzDecoder;
            use std::io::Read;

            let mut decoder = GzDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| checkpoint_error(format!("Failed to decompress data: {}", e)))?;
            decompressed
        } else {
            data.to_vec()
        };

        rmp_serde::from_slice(&msgpack_data)
            .map_err(|e| checkpoint_error(format!("Failed to deserialize checkpoint: {}", e)))
    }
}

#[async_trait]
impl<S: GraphState> Checkpointer<S> for PostgresCheckpointer
where
    S: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
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

        let checkpoint_data = self.serialize_checkpoint(&checkpoint)?;
        let metadata_json = serde_json::to_value(&metadata)?;
        let pending_writes_json = serde_json::to_value(&Vec::<PendingWrite>::new())?;
        let size_bytes = checkpoint_data.len() as i64;

        let query = format!(
            r#"
            INSERT INTO {} 
            (id, thread_id, checkpoint_data, metadata, pending_writes, step_number, size_bytes, compressed)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                checkpoint_data = EXCLUDED.checkpoint_data,
                metadata = EXCLUDED.metadata,
                pending_writes = EXCLUDED.pending_writes,
                step_number = EXCLUDED.step_number,
                size_bytes = EXCLUDED.size_bytes,
                compressed = EXCLUDED.compressed,
                created_at = NOW()
            "#,
            self.table_name()
        );

        sqlx::query(&query)
            .bind(&checkpoint.id)
            .bind(&config.thread_id)
            .bind(&checkpoint_data)
            .bind(&metadata_json)
            .bind(&pending_writes_json)
            .bind(checkpoint.step as i32)
            .bind(size_bytes)
            .bind(self.config.compress)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to store checkpoint {}: {}", checkpoint.id, e);
                checkpoint_error(format!("Failed to store checkpoint: {}", e))
            })?;

        debug!("Successfully stored checkpoint {}", checkpoint.id);
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

        let query = format!(
            r#"
            SELECT checkpoint_data, metadata, pending_writes, compressed
            FROM {} 
            WHERE id = $1 AND thread_id = $2
            "#,
            self.table_name()
        );

        let row = sqlx::query(&query)
            .bind(checkpoint_id)
            .bind(&config.thread_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to retrieve checkpoint {}: {}", checkpoint_id, e);
                checkpoint_error(format!("Failed to retrieve checkpoint: {}", e))
            })?;

        if let Some(row) = row {
            let checkpoint_data: Vec<u8> = row.get("checkpoint_data");
            let compressed: bool = row.get("compressed");
            let checkpoint = self.deserialize_checkpoint(&checkpoint_data, compressed)?;

            let metadata_json: serde_json::Value = row.get("metadata");
            let pending_writes_json: serde_json::Value = row.get("pending_writes");

            let metadata: CheckpointMetadata = serde_json::from_value(metadata_json)?;
            let pending_writes: Vec<PendingWrite> = serde_json::from_value(pending_writes_json)?;

            Ok(Some(CheckpointTuple {
                checkpoint,
                pending_writes,
                config: config.clone(),
            }))
        } else {
            debug!("Checkpoint {} not found", checkpoint_id);
            Ok(None)
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

        let query = format!(
            r#"
            SELECT id, checkpoint_data, metadata, pending_writes, compressed
            FROM {} 
            WHERE thread_id = $1
            ORDER BY step_number DESC, created_at DESC 
            LIMIT 1
            "#,
            self.table_name()
        );

        let row = sqlx::query(&query)
            .bind(&config.thread_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!(
                    "Failed to retrieve latest checkpoint for thread {}: {}",
                    config.thread_id, e
                );
                checkpoint_error(format!("Failed to retrieve latest checkpoint: {}", e))
            })?;

        if let Some(row) = row {
            let checkpoint_id: String = row.get("id");
            let checkpoint_data: Vec<u8> = row.get("checkpoint_data");
            let compressed: bool = row.get("compressed");
            let checkpoint = self.deserialize_checkpoint(&checkpoint_data, compressed)?;

            let metadata_json: serde_json::Value = row.get("metadata");
            let pending_writes_json: serde_json::Value = row.get("pending_writes");

            let metadata: CheckpointMetadata = serde_json::from_value(metadata_json)?;
            let pending_writes: Vec<PendingWrite> = serde_json::from_value(pending_writes_json)?;

            debug!(
                "Found latest checkpoint {} for thread {}",
                checkpoint_id, config.thread_id
            );
            Ok(Some(CheckpointTuple {
                checkpoint,
                pending_writes,
                config: config.clone(),
            }))
        } else {
            debug!("No checkpoints found for thread {}", config.thread_id);
            Ok(None)
        }
    }

    async fn list(
        &self,
        config: &CheckpointConfig,
        limit: Option<usize>,
        before: Option<&str>,
    ) -> GraphResult<Vec<CheckpointTuple<S>>> {
        debug!(
            "Listing checkpoints for thread {} (limit: {:?})",
            config.thread_id, limit
        );

        let mut query = format!(
            "SELECT id, checkpoint_data, metadata, pending_writes, compressed FROM {} WHERE thread_id = $1",
            self.table_name()
        );
        let mut param_count = 1;

        if let Some(before_id) = before {
            param_count += 1;
            query.push_str(&format!(
                " AND created_at < (SELECT created_at FROM {} WHERE id = ${})",
                self.table_name(),
                param_count
            ));
        }

        query.push_str(" ORDER BY step_number DESC, created_at DESC");

        if let Some(limit) = limit {
            param_count += 1;
            query.push_str(&format!(" LIMIT ${}", param_count));
        }

        let mut query_builder = sqlx::query(&query).bind(&config.thread_id);

        if let Some(before_id) = before {
            query_builder = query_builder.bind(before_id);
        }

        if let Some(limit) = limit {
            query_builder = query_builder.bind(limit as i64);
        }

        let rows = query_builder.fetch_all(&self.pool).await.map_err(|e| {
            error!(
                "Failed to list checkpoints for thread {}: {}",
                config.thread_id, e
            );
            checkpoint_error(format!("Failed to list checkpoints: {}", e))
        })?;

        let mut checkpoints = Vec::new();
        for row in rows {
            let checkpoint_data: Vec<u8> = row.get("checkpoint_data");
            let compressed: bool = row.get("compressed");
            let checkpoint = self.deserialize_checkpoint(&checkpoint_data, compressed)?;

            let metadata_json: serde_json::Value = row.get("metadata");
            let pending_writes_json: serde_json::Value = row.get("pending_writes");

            let metadata: CheckpointMetadata = serde_json::from_value(metadata_json)?;
            let pending_writes: Vec<PendingWrite> = serde_json::from_value(pending_writes_json)?;

            checkpoints.push(CheckpointTuple {
                checkpoint,
                pending_writes,
                config: config.clone(),
            });
        }

        debug!(
            "Found {} checkpoints for thread {}",
            checkpoints.len(),
            config.thread_id
        );
        Ok(checkpoints)
    }

    async fn delete(&self, config: &CheckpointConfig, checkpoint_id: &str) -> GraphResult<bool> {
        debug!(
            "Deleting checkpoint {} for thread {}",
            checkpoint_id, config.thread_id
        );

        let query = format!(
            "DELETE FROM {} WHERE id = $1 AND thread_id = $2",
            self.table_name()
        );

        let result = sqlx::query(&query)
            .bind(checkpoint_id)
            .bind(&config.thread_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to delete checkpoint {}: {}", checkpoint_id, e);
                checkpoint_error(format!("Failed to delete checkpoint: {}", e))
            })?;

        let deleted = result.rows_affected() > 0;
        if deleted {
            debug!("Successfully deleted checkpoint {}", checkpoint_id);
        } else {
            warn!("Checkpoint {} not found for deletion", checkpoint_id);
        }

        Ok(deleted)
    }

    async fn clear_thread(&self, thread_id: &str) -> GraphResult<usize> {
        debug!("Clearing all checkpoints for thread {}", thread_id);

        let query = format!("DELETE FROM {} WHERE thread_id = $1", self.table_name());

        let result = sqlx::query(&query)
            .bind(thread_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to clear thread {}: {}", thread_id, e);
                checkpoint_error(format!("Failed to clear thread: {}", e))
            })?;

        let deleted = result.rows_affected() as usize;
        info!("Cleared {} checkpoints for thread {}", deleted, thread_id);
        Ok(deleted)
    }

    async fn stats(&self) -> GraphResult<CheckpointerStats> {
        debug!("Gathering checkpointer statistics");

        let query = format!(
            r#"
            SELECT 
                COUNT(*) as total,
                COUNT(DISTINCT thread_id) as unique_threads,
                COALESCE(SUM(size_bytes), 0) as storage_size,
                COALESCE(AVG(size_bytes), 0) as avg_size,
                MIN(created_at) as oldest,
                MAX(created_at) as newest
            FROM {}
            "#,
            self.table_name()
        );

        let stats_row = sqlx::query(&query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to gather statistics: {}", e);
                checkpoint_error(format!("Failed to gather statistics: {}", e))
            })?;

        let total_checkpoints = stats_row.get::<i64, _>("total") as usize;
        let unique_threads = stats_row.get::<i64, _>("unique_threads") as usize;
        let storage_size_bytes = stats_row.get::<i64, _>("storage_size") as u64;
        let avg_checkpoint_size_bytes = stats_row.get::<f64, _>("avg_size") as u64;

        let oldest_checkpoint = stats_row.get::<Option<DateTime<Utc>>, _>("oldest");
        let newest_checkpoint = stats_row.get::<Option<DateTime<Utc>>, _>("newest");

        let stats = CheckpointerStats {
            total_checkpoints,
            unique_threads,
            storage_size_bytes,
            avg_checkpoint_size_bytes,
            oldest_checkpoint,
            newest_checkpoint,
        };

        debug!(
            "Statistics: {} total checkpoints, {} unique threads, {} bytes storage",
            stats.total_checkpoints, stats.unique_threads, stats.storage_size_bytes
        );

        Ok(stats)
    }

    async fn cleanup(&self) -> GraphResult<CleanupResult> {
        let start_time = std::time::Instant::now();
        info!("Starting cleanup operation");

        // Delete checkpoints older than configured days
        let cutoff_date = Utc::now() - chrono::Duration::days(self.config.cleanup_days as i64);

        // Get size of checkpoints to be deleted
        let size_query = format!(
            "SELECT COALESCE(SUM(size_bytes), 0) as total_size FROM {} WHERE created_at < $1",
            self.table_name()
        );

        let size_row = sqlx::query(&size_query)
            .bind(cutoff_date)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| checkpoint_error(format!("Failed to calculate cleanup size: {}", e)))?;

        let space_freed_bytes = size_row.get::<i64, _>("total_size") as u64;

        // Delete old checkpoints
        let delete_query = format!("DELETE FROM {} WHERE created_at < $1", self.table_name());

        let result = sqlx::query(&delete_query)
            .bind(cutoff_date)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to cleanup old checkpoints: {}", e);
                checkpoint_error(format!("Failed to cleanup: {}", e))
            })?;

        let checkpoints_removed = result.rows_affected() as usize;
        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Run VACUUM ANALYZE for PostgreSQL optimization
        let vacuum_query = format!("VACUUM ANALYZE {}", self.table_name());
        sqlx::query(&vacuum_query)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                warn!("Failed to run VACUUM ANALYZE: {}", e);
                checkpoint_error(format!("Failed to vacuum database: {}", e))
            })?;

        let cleanup_result = CleanupResult {
            checkpoints_removed,
            space_freed_bytes,
            duration_ms,
        };

        info!(
            "Cleanup completed: removed {} checkpoints, freed {} bytes in {}ms",
            cleanup_result.checkpoints_removed,
            cleanup_result.space_freed_bytes,
            cleanup_result.duration_ms
        );

        Ok(cleanup_result)
    }
}

impl Default for PostgresCheckpointer {
    fn default() -> Self {
        // This will panic if called - use new() or with_config() instead
        panic!("PostgresCheckpointer::default() is not supported - use PostgresCheckpointer::new() instead")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use langgraph_core::GraphState;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestState {
        value: i32,
    }

    impl GraphState for TestState {}

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database"]
    async fn test_postgres_checkpointer() {
        let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:password@localhost/test_checkpoints".to_string()
        });

        let checkpointer = PostgresCheckpointer::new(database_url).await.unwrap();
        let config = CheckpointConfig::default();

        let state = TestState { value: 42 };
        let checkpoint = Checkpoint::new("test-1".to_string(), state, 1);
        let metadata = CheckpointMetadata::new();

        // Test put
        checkpointer
            .put(&config, checkpoint.clone(), metadata)
            .await
            .unwrap();

        // Test get
        let result = checkpointer.get(&config, "test-1").await.unwrap();
        assert!(result.is_some());
        let tuple = result.unwrap();
        assert_eq!(tuple.checkpoint.state.value, 42);

        // Test get_latest
        let latest = checkpointer.get_latest(&config).await.unwrap();
        assert!(latest.is_some());

        // Test list
        let list = checkpointer.list(&config, Some(10), None).await.unwrap();
        assert_eq!(list.len(), 1);

        // Test stats
        let stats = checkpointer.stats().await.unwrap();
        assert_eq!(stats.total_checkpoints, 1);

        // Test delete
        let deleted = checkpointer.delete(&config, "test-1").await.unwrap();
        assert!(deleted);

        // Verify deletion
        let result = checkpointer.get(&config, "test-1").await.unwrap();
        assert!(result.is_none());
    }
}
