//! Velkor retention — background job that auto-deletes expired conversations
//! and messages based on configured retention policies.

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Default retention period in days for conversations with no explicit policy.
const DEFAULT_RETENTION_DAYS: i64 = 90;

/// Configuration for the retention background task.
#[derive(Debug, Clone)]
pub struct RetentionConfig {
    /// How often to run the sweep (in seconds).
    pub interval_secs: u64,
    /// Default retention period for conversations (days).
    pub default_retention_days: i64,
    /// Whether to hard-delete or just soft-delete (mark is_deleted).
    pub hard_delete: bool,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            interval_secs: 3600, // hourly
            default_retention_days: DEFAULT_RETENTION_DAYS,
            hard_delete: false,
        }
    }
}

/// Shared status of the retention background task.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionStatus {
    pub config: RetentionStatusConfig,
    pub last_sweep_at: Option<DateTime<Utc>>,
    pub last_sweep_deleted: u64,
    pub total_sweeps: u64,
    pub total_deleted: u64,
    pub running: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionStatusConfig {
    pub interval_secs: u64,
    pub default_retention_days: i64,
    pub hard_delete: bool,
}

/// Thread-safe handle to retention status, shared with the rest of the app.
pub type RetentionStatusHandle = Arc<RwLock<RetentionStatus>>;

/// Create a new status handle with the given config.
pub fn new_status_handle(config: &RetentionConfig) -> RetentionStatusHandle {
    Arc::new(RwLock::new(RetentionStatus {
        config: RetentionStatusConfig {
            interval_secs: config.interval_secs,
            default_retention_days: config.default_retention_days,
            hard_delete: config.hard_delete,
        },
        last_sweep_at: None,
        last_sweep_deleted: 0,
        total_sweeps: 0,
        total_deleted: 0,
        running: true,
    }))
}

/// Run the retention sweep once — soft-deletes expired conversations and their
/// messages. Returns the number of conversations affected.
pub async fn sweep(pool: &PgPool, config: &RetentionConfig) -> anyhow::Result<u64> {
    let cutoff = Utc::now() - Duration::days(config.default_retention_days);

    if config.hard_delete {
        // Hard delete: remove messages first (FK), then conversations
        let msg_result = sqlx::query(
            "DELETE FROM messages WHERE conversation_id IN (SELECT id FROM conversations WHERE started_at < $1 AND is_deleted = FALSE)",
        )
        .bind(cutoff)
        .execute(pool)
        .await?;

        let conv_result = sqlx::query(
            "DELETE FROM conversations WHERE started_at < $1 AND is_deleted = FALSE",
        )
        .bind(cutoff)
        .execute(pool)
        .await?;

        let count = conv_result.rows_affected();
        if count > 0 {
            info!(
                conversations = count,
                messages = msg_result.rows_affected(),
                cutoff = %cutoff,
                "Retention sweep: hard-deleted expired records"
            );
        }
        Ok(count)
    } else {
        // Soft delete: mark conversations as deleted
        let result = sqlx::query(
            "UPDATE conversations SET is_deleted = TRUE, ended_at = COALESCE(ended_at, now()) WHERE started_at < $1 AND is_deleted = FALSE",
        )
        .bind(cutoff)
        .execute(pool)
        .await?;

        let count = result.rows_affected();
        if count > 0 {
            info!(
                conversations = count,
                cutoff = %cutoff,
                "Retention sweep: soft-deleted expired conversations"
            );
        }
        Ok(count)
    }
}

/// Spawn the retention background task. Runs indefinitely, sweeping at the
/// configured interval. Updates the shared status handle after each sweep.
pub fn spawn_retention_task(
    pool: PgPool,
    config: RetentionConfig,
    status: RetentionStatusHandle,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            interval_secs = config.interval_secs,
            retention_days = config.default_retention_days,
            "Retention background task started"
        );

        let mut interval = tokio::time::interval(
            std::time::Duration::from_secs(config.interval_secs),
        );

        // Skip the first immediate tick
        interval.tick().await;

        loop {
            interval.tick().await;
            match sweep(&pool, &config).await {
                Ok(count) => {
                    let mut s = status.write().await;
                    s.last_sweep_at = Some(Utc::now());
                    s.last_sweep_deleted = count;
                    s.total_sweeps += 1;
                    s.total_deleted += count;
                    if count > 0 {
                        info!(expired = count, "Retention sweep completed");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Retention sweep failed");
                }
            }
        }
    })
}
