//! Velkor Pulse — the autonomous background engine of the platform.
//!
//! Per PRD Section 5.2: a single Tokio-based timer runs independently of user
//! interaction. Every tick (configurable, default 60s) it drives all autonomous
//! background work through a set of pluggable subsystems:
//!
//! - **Scheduler**: check for due cron tasks, spawn agent execution
//! - **Triggers**: check monitored sources (file watchers, webhook queues, email)
//! - **Maintenance**: memory summarization, expired memory cleanup
//! - **Retention**: soft/hard delete of aged conversations
//!
//! Each subsystem implements `PulseSubsystem` and is registered with the
//! pulse engine at startup. The engine runs them sequentially each tick,
//! collects their results, and updates a shared status handle.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Pulse configuration loaded from platform YAML.
#[derive(Debug, Clone)]
pub struct PulseConfig {
    /// Whether the pulse engine is enabled at all.
    pub enabled: bool,
    /// Base tick interval in seconds.
    pub interval_secs: u64,
}

impl Default for PulseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 60,
        }
    }
}

// ---------------------------------------------------------------------------
// Subsystem trait
// ---------------------------------------------------------------------------

/// Result of a single subsystem tick.
#[derive(Debug, Clone, Serialize)]
pub struct SubsystemTickResult {
    /// Name of the subsystem that ran.
    pub name: String,
    /// How many items were due / found / checked.
    pub checked: u32,
    /// How many items were successfully processed.
    pub processed: u32,
    /// How many items failed.
    pub failed: u32,
    /// How long the subsystem tick took.
    pub duration_ms: u64,
    /// Optional details (e.g. error messages, schedule names).
    pub details: Option<String>,
}

/// A pluggable subsystem that runs on each pulse tick.
///
/// Implementations register themselves with the pulse engine at startup.
/// The engine calls `tick()` on each subsystem every pulse interval.
/// Subsystems should be fast — long-running work (like agent execution) should
/// be spawned into background tasks, not run inline.
#[async_trait]
pub trait PulseSubsystem: Send + Sync {
    /// Human-readable name of this subsystem (e.g. "scheduler", "retention").
    fn name(&self) -> &str;

    /// Whether this subsystem should run on this tick.
    /// Called before `tick()`. Allows subsystems to run at a different cadence
    /// than the pulse (e.g. retention runs hourly, not every 60s).
    fn should_run(&self, tick_number: u64, interval_secs: u64) -> bool {
        // Default: run every tick
        let _ = (tick_number, interval_secs);
        true
    }

    /// Execute one tick of this subsystem's work.
    async fn tick(&self) -> anyhow::Result<SubsystemTickResult>;
}

// ---------------------------------------------------------------------------
// Pulse status
// ---------------------------------------------------------------------------

/// Live status of the pulse engine, exposed via admin API.
#[derive(Debug, Clone, Serialize)]
pub struct PulseStatus {
    pub enabled: bool,
    pub interval_secs: u64,
    pub running: bool,
    pub total_ticks: u64,
    pub last_tick_at: Option<DateTime<Utc>>,
    pub last_tick_duration_ms: u64,
    pub subsystems: Vec<SubsystemStatus>,
}

/// Status of a single subsystem within the pulse engine.
#[derive(Debug, Clone, Serialize)]
pub struct SubsystemStatus {
    pub name: String,
    pub enabled: bool,
    pub total_runs: u64,
    pub total_processed: u64,
    pub total_failed: u64,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_result: Option<SubsystemTickResult>,
}

pub type PulseStatusHandle = Arc<RwLock<PulseStatus>>;

/// Create a new pulse status handle.
pub fn new_status_handle(config: &PulseConfig) -> PulseStatusHandle {
    Arc::new(RwLock::new(PulseStatus {
        enabled: config.enabled,
        interval_secs: config.interval_secs,
        running: false,
        total_ticks: 0,
        last_tick_at: None,
        last_tick_duration_ms: 0,
        subsystems: vec![],
    }))
}

// ---------------------------------------------------------------------------
// Pulse engine
// ---------------------------------------------------------------------------

/// The pulse engine. Holds registered subsystems and drives the tick loop.
pub struct PulseEngine {
    config: PulseConfig,
    subsystems: Vec<Box<dyn PulseSubsystem>>,
    status: PulseStatusHandle,
}

impl PulseEngine {
    /// Create a new pulse engine.
    pub fn new(config: PulseConfig, status: PulseStatusHandle) -> Self {
        Self {
            config,
            subsystems: Vec::new(),
            status,
        }
    }

    /// Register a subsystem. Order matters — subsystems tick in registration order.
    pub fn register(&mut self, subsystem: Box<dyn PulseSubsystem>) {
        let name = subsystem.name().to_string();
        info!(subsystem = %name, "Registered pulse subsystem");
        self.subsystems.push(subsystem);
    }

    /// Spawn the pulse engine as a background tokio task.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Run the pulse loop (blocks forever).
    async fn run(self) {
        if !self.config.enabled {
            info!("Pulse engine disabled via config");
            let mut s = self.status.write().await;
            s.running = false;
            return;
        }

        // Initialize subsystem status entries
        {
            let mut s = self.status.write().await;
            s.running = true;
            s.subsystems = self
                .subsystems
                .iter()
                .map(|sub| SubsystemStatus {
                    name: sub.name().to_string(),
                    enabled: true,
                    total_runs: 0,
                    total_processed: 0,
                    total_failed: 0,
                    last_run_at: None,
                    last_result: None,
                })
                .collect();
        }

        info!(
            interval_secs = self.config.interval_secs,
            subsystems = self.subsystems.len(),
            names = ?self.subsystems.iter().map(|s| s.name()).collect::<Vec<_>>(),
            "Pulse engine started"
        );

        let mut interval = tokio::time::interval(
            std::time::Duration::from_secs(self.config.interval_secs),
        );

        // Skip the first immediate tick to let the system fully start
        interval.tick().await;

        let mut tick_number: u64 = 0;

        loop {
            interval.tick().await;
            tick_number += 1;

            let tick_start = std::time::Instant::now();
            let mut any_work = false;

            for (idx, subsystem) in self.subsystems.iter().enumerate() {
                if !subsystem.should_run(tick_number, self.config.interval_secs) {
                    continue;
                }

                let sub_start = std::time::Instant::now();
                let result = subsystem.tick().await;
                let sub_elapsed = sub_start.elapsed().as_millis() as u64;

                match result {
                    Ok(tick_result) => {
                        if tick_result.checked > 0 || tick_result.processed > 0 {
                            any_work = true;
                            debug!(
                                subsystem = subsystem.name(),
                                checked = tick_result.checked,
                                processed = tick_result.processed,
                                failed = tick_result.failed,
                                duration_ms = sub_elapsed,
                                "Subsystem tick completed"
                            );
                        }

                        // Update subsystem status
                        let mut s = self.status.write().await;
                        if let Some(ss) = s.subsystems.get_mut(idx) {
                            ss.total_runs += 1;
                            ss.total_processed += tick_result.processed as u64;
                            ss.total_failed += tick_result.failed as u64;
                            ss.last_run_at = Some(Utc::now());
                            ss.last_result = Some(tick_result);
                        }
                    }
                    Err(e) => {
                        warn!(
                            subsystem = subsystem.name(),
                            error = %e,
                            "Subsystem tick failed"
                        );

                        let mut s = self.status.write().await;
                        if let Some(ss) = s.subsystems.get_mut(idx) {
                            ss.total_runs += 1;
                            ss.total_failed += 1;
                            ss.last_run_at = Some(Utc::now());
                            ss.last_result = Some(SubsystemTickResult {
                                name: subsystem.name().to_string(),
                                checked: 0,
                                processed: 0,
                                failed: 1,
                                duration_ms: sub_elapsed,
                                details: Some(format!("Error: {e}")),
                            });
                        }
                    }
                }
            }

            let tick_elapsed = tick_start.elapsed().as_millis() as u64;

            // Update global pulse status
            {
                let mut s = self.status.write().await;
                s.total_ticks += 1;
                s.last_tick_at = Some(Utc::now());
                s.last_tick_duration_ms = tick_elapsed;
            }

            if any_work {
                info!(
                    tick = tick_number,
                    duration_ms = tick_elapsed,
                    "Pulse tick completed with work"
                );
            }
        }
    }
}
