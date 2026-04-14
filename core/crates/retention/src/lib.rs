//! Velkor retention — auto-deletes expired conversations and messages
//! based on configured retention policies.
//!
//! Implements `PulseSubsystem` so the unified pulse engine drives retention sweeps.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use tracing::info;
use velkor_pulse::{PulseSubsystem, SubsystemTickResult};

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

// ---------------------------------------------------------------------------
// PulseSubsystem implementation
// ---------------------------------------------------------------------------

/// Retention as a pulse subsystem. Runs at a configurable cadence
/// (default: every 60 ticks = hourly at 60s base interval).
pub struct RetentionSubsystem {
    pool: PgPool,
    config: RetentionConfig,
    /// How many ticks between retention sweeps.
    /// E.g. if pulse interval is 60s and every_n_ticks is 60, retention runs hourly.
    every_n_ticks: u64,
}

impl RetentionSubsystem {
    pub fn new(pool: PgPool, config: RetentionConfig, pulse_interval_secs: u64) -> Self {
        // Calculate how many ticks correspond to the retention interval
        let every_n_ticks = if pulse_interval_secs > 0 {
            config.interval_secs / pulse_interval_secs
        } else {
            1
        }
        .max(1);

        Self {
            pool,
            config,
            every_n_ticks,
        }
    }
}

#[async_trait]
impl PulseSubsystem for RetentionSubsystem {
    fn name(&self) -> &str {
        "retention"
    }

    fn should_run(&self, tick_number: u64, _interval_secs: u64) -> bool {
        tick_number % self.every_n_ticks == 0
    }

    async fn tick(&self) -> anyhow::Result<SubsystemTickResult> {
        let deleted = sweep(&self.pool, &self.config).await?;
        Ok(SubsystemTickResult {
            name: "retention".to_string(),
            checked: 1, // one sweep operation
            processed: deleted as u32,
            failed: 0,
            duration_ms: 0, // engine fills this in
            details: if deleted > 0 {
                Some(format!("Swept {} expired conversations", deleted))
            } else {
                None
            },
        })
    }
}
