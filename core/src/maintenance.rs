//! Memory maintenance subsystem for the Pulse engine.
//!
//! Periodic background tasks:
//! 1. Expire old low-importance memories (importance < 5, older than 90 days)
//! 2. Hard-delete memories that have been soft-deleted for > 30 days

use async_trait::async_trait;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use tracing::{debug, info};
use velkor_pulse::{PulseSubsystem, SubsystemTickResult};

/// Memory maintenance as a pulse subsystem.
/// Runs infrequently (every ~6 hours at default 60s interval).
pub struct MemoryMaintenanceSubsystem {
    pool: PgPool,
    /// How many pulse ticks between maintenance runs.
    every_n_ticks: u64,
}

impl MemoryMaintenanceSubsystem {
    pub fn new(pool: PgPool, pulse_interval_secs: u64) -> Self {
        // Run every ~6 hours
        let target_interval_secs = 6 * 3600;
        let every_n_ticks = if pulse_interval_secs > 0 {
            target_interval_secs / pulse_interval_secs
        } else {
            1
        }
        .max(1);

        Self {
            pool,
            every_n_ticks,
        }
    }
}

#[async_trait]
impl PulseSubsystem for MemoryMaintenanceSubsystem {
    fn name(&self) -> &str {
        "memory_maintenance"
    }

    fn should_run(&self, tick_number: u64, _interval_secs: u64) -> bool {
        tick_number % self.every_n_ticks == 0
    }

    async fn tick(&self) -> anyhow::Result<SubsystemTickResult> {
        let mut total_processed = 0u32;

        // 1. Expire old low-importance memories (importance < 5, older than 90 days)
        let cutoff_low = Utc::now() - Duration::days(90);
        let expired = sqlx::query(
            "UPDATE memories SET is_deleted = TRUE, deleted_at = now() \
             WHERE NOT is_deleted AND importance < 5 AND created_at < $1",
        )
        .bind(cutoff_low)
        .execute(&self.pool)
        .await?;

        let expired_count = expired.rows_affected() as u32;
        total_processed += expired_count;

        if expired_count > 0 {
            info!(count = expired_count, "Expired old low-importance memories");
        }

        // 2. Hard-delete memories soft-deleted > 30 days ago (cleanup)
        let purge_cutoff = Utc::now() - Duration::days(30);
        let purged = sqlx::query(
            "DELETE FROM memories WHERE is_deleted = TRUE AND deleted_at < $1",
        )
        .bind(purge_cutoff)
        .execute(&self.pool)
        .await?;

        let purged_count = purged.rows_affected() as u32;
        total_processed += purged_count;

        if purged_count > 0 {
            info!(count = purged_count, "Purged old soft-deleted memories");
        }

        // 3. Expire memories past their explicit expires_at
        let now = Utc::now();
        let ttl_expired = sqlx::query(
            "UPDATE memories SET is_deleted = TRUE, deleted_at = now() \
             WHERE NOT is_deleted AND expires_at IS NOT NULL AND expires_at < $1",
        )
        .bind(now)
        .execute(&self.pool)
        .await?;

        let ttl_count = ttl_expired.rows_affected() as u32;
        total_processed += ttl_count;

        if ttl_count > 0 {
            info!(count = ttl_count, "Expired memories past TTL");
        }

        debug!(
            expired = expired_count,
            purged = purged_count,
            ttl = ttl_count,
            "Memory maintenance tick"
        );

        Ok(SubsystemTickResult {
            name: "memory_maintenance".to_string(),
            checked: 3, // 3 maintenance operations
            processed: total_processed,
            failed: 0,
            duration_ms: 0,
            details: if total_processed > 0 {
                Some(format!(
                    "Expired: {}, purged: {}, TTL: {}",
                    expired_count, purged_count, ttl_count
                ))
            } else {
                None
            },
        })
    }
}
