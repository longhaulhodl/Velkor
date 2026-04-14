//! Velkor scheduler — heartbeat-driven cron scheduler for autonomous agent tasks.
//!
//! Per PRD Section 5.2: a Tokio-based timer runs independently of user interaction.
//! Every tick (configurable, default 60s):
//! 1. Check for due scheduled tasks (next_run_at <= now, is_active = true)
//! 2. Spawn agent execution for each due task
//! 3. Record the run in schedule_runs with status, tokens, cost
//! 4. Update schedule.last_run_at and compute schedule.next_run_at

use chrono::{DateTime, Utc};
use cron::Schedule as CronSchedule;
use serde::Serialize;
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use velkor_runtime::context::ConversationContext;
use velkor_runtime::react::AgentRuntime;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Scheduler configuration loaded from the platform YAML.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Whether the scheduler is enabled.
    pub enabled: bool,
    /// Heartbeat interval in seconds (how often we check for due tasks).
    pub heartbeat_secs: u64,
    /// Timezone for cron evaluation (e.g. "America/Chicago"). Defaults to UTC.
    pub timezone: Option<String>,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            heartbeat_secs: 60,
            timezone: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Status (shared handle, like retention)
// ---------------------------------------------------------------------------

/// Live status of the scheduler background task.
#[derive(Debug, Clone, Serialize)]
pub struct SchedulerStatus {
    pub enabled: bool,
    pub heartbeat_secs: u64,
    pub last_tick_at: Option<DateTime<Utc>>,
    pub last_tick_due: u32,
    pub last_tick_executed: u32,
    pub total_ticks: u64,
    pub total_runs: u64,
    pub total_failures: u64,
    pub running: bool,
}

pub type SchedulerStatusHandle = Arc<RwLock<SchedulerStatus>>;

pub fn new_status_handle(config: &SchedulerConfig) -> SchedulerStatusHandle {
    Arc::new(RwLock::new(SchedulerStatus {
        enabled: config.enabled,
        heartbeat_secs: config.heartbeat_secs,
        last_tick_at: None,
        last_tick_due: 0,
        last_tick_executed: 0,
        total_ticks: 0,
        total_runs: 0,
        total_failures: 0,
        running: config.enabled,
    }))
}

// ---------------------------------------------------------------------------
// Schedule row from DB
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct ScheduleRow {
    id: Uuid,
    user_id: Uuid,
    agent_id: String,
    name: String,
    cron_expression: String,
    task_prompt: String,
}

// ---------------------------------------------------------------------------
// Core: compute next run from cron expression
// ---------------------------------------------------------------------------

/// Parse a cron expression and compute the next occurrence after `after`.
/// Uses the 7-field cron format (sec min hour dom month dow year).
/// Accepts standard 5-field format by prepending "0 " (sec=0) and appending " *" (year=any).
pub fn next_run_after(cron_expr: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    // Normalize 5-field cron to 7-field by adding seconds and year
    let normalized = match cron_expr.split_whitespace().count() {
        5 => format!("0 {} *", cron_expr),
        6 => format!("0 {}", cron_expr),
        7 => cron_expr.to_string(),
        _ => return None,
    };

    let schedule = CronSchedule::from_str(&normalized).ok()?;
    schedule.after(&after).next()
}

// ---------------------------------------------------------------------------
// Core: execute a single scheduled task
// ---------------------------------------------------------------------------

/// Execute a scheduled task: create conversation, run agent, record result.
async fn execute_schedule(
    pool: &PgPool,
    runtime: &AgentRuntime,
    schedule: &ScheduleRow,
) -> anyhow::Result<()> {
    let conversation_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();

    // Insert the schedule_run record as 'running'
    sqlx::query(
        "INSERT INTO schedule_runs (id, schedule_id, started_at, status, conversation_id) \
         VALUES ($1, $2, now(), 'running', $3)",
    )
    .bind(run_id)
    .bind(schedule.id)
    .bind(conversation_id)
    .execute(pool)
    .await?;

    // Create a conversation record for this scheduled run
    let _ = sqlx::query(
        "INSERT INTO conversations (id, user_id, agent_id, title, started_at) \
         VALUES ($1, $2, $3, $4, now())",
    )
    .bind(conversation_id)
    .bind(schedule.user_id)
    .bind(&schedule.agent_id)
    .bind(format!("[Scheduled] {}", schedule.name))
    .execute(pool)
    .await;

    // Persist the user message (the task prompt)
    let msg_id = Uuid::new_v4();
    let _ = sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) \
         VALUES ($1, $2, 'user', $3, now())",
    )
    .bind(msg_id)
    .bind(conversation_id)
    .bind(&schedule.task_prompt)
    .execute(pool)
    .await;

    // Build conversation context and run the agent
    let mut context = ConversationContext::new(
        conversation_id,
        schedule.user_id,
        &schedule.agent_id,
    );

    let result = runtime.run(&schedule.task_prompt, &mut context).await;

    match result {
        Ok(response) => {
            // Persist assistant response
            let resp_id = Uuid::new_v4();
            let _ = sqlx::query(
                "INSERT INTO messages (id, conversation_id, role, content, created_at) \
                 VALUES ($1, $2, 'assistant', $3, now())",
            )
            .bind(resp_id)
            .bind(conversation_id)
            .bind(&response.content)
            .execute(pool)
            .await;

            let total_tokens = response.usage.input_tokens + response.usage.output_tokens;
            let summary = if response.content.len() > 500 {
                format!("{}...", &response.content[..500])
            } else {
                response.content.clone()
            };

            // Update run as completed
            sqlx::query(
                "UPDATE schedule_runs SET status = 'completed', completed_at = now(), \
                 result_summary = $1, tokens_used = $2 \
                 WHERE id = $3",
            )
            .bind(&summary)
            .bind(total_tokens as i32)
            .bind(run_id)
            .execute(pool)
            .await?;

            // Update schedule stats
            sqlx::query(
                "UPDATE schedules SET last_run_at = now(), run_count = run_count + 1, \
                 last_error = NULL WHERE id = $1",
            )
            .bind(schedule.id)
            .execute(pool)
            .await?;

            info!(
                schedule = %schedule.name,
                schedule_id = %schedule.id,
                conversation_id = %conversation_id,
                tokens = total_tokens,
                iterations = response.iterations,
                "Scheduled task completed successfully"
            );

            Ok(())
        }
        Err(e) => {
            let err_msg = format!("{e}");

            // Update run as failed
            sqlx::query(
                "UPDATE schedule_runs SET status = 'failed', completed_at = now(), \
                 error = $1 WHERE id = $2",
            )
            .bind(&err_msg)
            .bind(run_id)
            .execute(pool)
            .await?;

            // Update schedule error tracking
            sqlx::query(
                "UPDATE schedules SET last_run_at = now(), run_count = run_count + 1, \
                 error_count = error_count + 1, last_error = $1 WHERE id = $2",
            )
            .bind(&err_msg)
            .bind(schedule.id)
            .execute(pool)
            .await?;

            error!(
                schedule = %schedule.name,
                schedule_id = %schedule.id,
                error = %e,
                "Scheduled task failed"
            );

            Err(e.into())
        }
    }
}

// ---------------------------------------------------------------------------
// Heartbeat tick: check for due tasks and execute them
// ---------------------------------------------------------------------------

/// Run one heartbeat tick: query for due schedules and execute them.
/// Returns (due_count, executed_count, failed_count).
async fn tick(
    pool: &PgPool,
    runtime: &AgentRuntime,
) -> anyhow::Result<(u32, u32, u32)> {
    let now = Utc::now();

    // Fetch all active schedules that are due
    let due_schedules = sqlx::query_as::<_, ScheduleRow>(
        "SELECT id, user_id, agent_id, name, cron_expression, task_prompt \
         FROM schedules \
         WHERE is_active = TRUE AND next_run_at <= $1 \
         ORDER BY next_run_at ASC \
         LIMIT 50",
    )
    .bind(now)
    .fetch_all(pool)
    .await?;

    let due_count = due_schedules.len() as u32;
    if due_count == 0 {
        return Ok((0, 0, 0));
    }

    debug!(due = due_count, "Scheduler heartbeat: found due tasks");

    let mut executed = 0u32;
    let mut failed = 0u32;

    for schedule in &due_schedules {
        // Compute next_run_at BEFORE executing, so we don't re-trigger on the next tick
        let next = next_run_after(&schedule.cron_expression, now);
        if let Some(next_at) = next {
            sqlx::query("UPDATE schedules SET next_run_at = $1 WHERE id = $2")
                .bind(next_at)
                .bind(schedule.id)
                .execute(pool)
                .await?;
        } else {
            warn!(
                schedule = %schedule.name,
                cron = %schedule.cron_expression,
                "Failed to compute next_run_at — deactivating schedule"
            );
            sqlx::query("UPDATE schedules SET is_active = FALSE, last_error = 'Invalid cron expression' WHERE id = $1")
                .bind(schedule.id)
                .execute(pool)
                .await?;
            continue;
        }

        // Execute the scheduled task
        match execute_schedule(pool, runtime, schedule).await {
            Ok(()) => executed += 1,
            Err(e) => {
                warn!(schedule = %schedule.name, error = %e, "Schedule execution failed");
                failed += 1;
            }
        }
    }

    Ok((due_count, executed, failed))
}

// ---------------------------------------------------------------------------
// Background task: the heartbeat loop
// ---------------------------------------------------------------------------

/// Spawn the scheduler heartbeat as a background tokio task.
///
/// Runs indefinitely, checking for due tasks at the configured interval.
/// Updates the shared status handle after each tick.
pub fn spawn_scheduler_task(
    pool: PgPool,
    config: SchedulerConfig,
    runtime: Arc<AgentRuntime>,
    status: SchedulerStatusHandle,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if !config.enabled {
            info!("Scheduler disabled via config");
            let mut s = status.write().await;
            s.running = false;
            return;
        }

        info!(
            heartbeat_secs = config.heartbeat_secs,
            timezone = ?config.timezone,
            "Scheduler heartbeat started"
        );

        // On startup, initialize next_run_at for any active schedules that are NULL
        if let Err(e) = initialize_next_runs(&pool).await {
            warn!(error = %e, "Failed to initialize next_run_at on startup");
        }

        let mut interval = tokio::time::interval(
            std::time::Duration::from_secs(config.heartbeat_secs),
        );

        // Skip the first immediate tick to let the system fully start
        interval.tick().await;

        loop {
            interval.tick().await;

            match tick(&pool, &runtime).await {
                Ok((due, executed, failed)) => {
                    let mut s = status.write().await;
                    s.last_tick_at = Some(Utc::now());
                    s.last_tick_due = due;
                    s.last_tick_executed = executed;
                    s.total_ticks += 1;
                    s.total_runs += executed as u64;
                    s.total_failures += failed as u64;

                    if due > 0 {
                        info!(
                            due = due,
                            executed = executed,
                            failed = failed,
                            "Scheduler tick completed"
                        );
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Scheduler tick failed");
                    let mut s = status.write().await;
                    s.last_tick_at = Some(Utc::now());
                    s.total_ticks += 1;
                }
            }
        }
    })
}

/// On startup, set next_run_at for any active schedule where it's NULL.
async fn initialize_next_runs(pool: &PgPool) -> anyhow::Result<()> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, cron_expression FROM schedules WHERE is_active = TRUE AND next_run_at IS NULL",
    )
    .fetch_all(pool)
    .await?;

    let now = Utc::now();
    let mut updated = 0u32;

    for (id, cron_expr) in &rows {
        if let Some(next) = next_run_after(cron_expr, now) {
            sqlx::query("UPDATE schedules SET next_run_at = $1 WHERE id = $2")
                .bind(next)
                .bind(id)
                .execute(pool)
                .await?;
            updated += 1;
        } else {
            warn!(schedule_id = %id, cron = %cron_expr, "Invalid cron — deactivating");
            sqlx::query("UPDATE schedules SET is_active = FALSE, last_error = 'Invalid cron expression' WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await?;
        }
    }

    if updated > 0 {
        info!(count = updated, "Initialized next_run_at for active schedules");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// CRUD helpers (called from route handlers)
// ---------------------------------------------------------------------------

/// Row returned from schedule listing queries.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ScheduleInfo {
    pub id: Uuid,
    pub user_id: Uuid,
    pub agent_id: String,
    pub name: String,
    pub description: Option<String>,
    pub cron_expression: String,
    pub natural_language: Option<String>,
    pub task_prompt: String,
    pub delivery_channel: Option<String>,
    pub delivery_target: Option<String>,
    pub is_active: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub run_count: i32,
    pub error_count: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Row returned from schedule run history queries.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ScheduleRunInfo {
    pub id: Uuid,
    pub schedule_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: Option<String>,
    pub result_summary: Option<String>,
    pub conversation_id: Option<Uuid>,
    pub tokens_used: Option<i32>,
    pub cost_usd: Option<f64>,
    pub error: Option<String>,
}

/// List all schedules for a user (or all if admin).
pub async fn list_schedules(pool: &PgPool, user_id: Option<Uuid>) -> anyhow::Result<Vec<ScheduleInfo>> {
    let rows = if let Some(uid) = user_id {
        sqlx::query_as::<_, ScheduleInfo>(
            "SELECT id, user_id, agent_id, name, description, cron_expression, natural_language, \
             task_prompt, delivery_channel, delivery_target, is_active, last_run_at, next_run_at, \
             run_count, error_count, last_error, created_at \
             FROM schedules WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(uid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, ScheduleInfo>(
            "SELECT id, user_id, agent_id, name, description, cron_expression, natural_language, \
             task_prompt, delivery_channel, delivery_target, is_active, last_run_at, next_run_at, \
             run_count, error_count, last_error, created_at \
             FROM schedules ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

/// Get a single schedule by ID.
pub async fn get_schedule(pool: &PgPool, id: Uuid) -> anyhow::Result<Option<ScheduleInfo>> {
    let row = sqlx::query_as::<_, ScheduleInfo>(
        "SELECT id, user_id, agent_id, name, description, cron_expression, natural_language, \
         task_prompt, delivery_channel, delivery_target, is_active, last_run_at, next_run_at, \
         run_count, error_count, last_error, created_at \
         FROM schedules WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Create a new schedule. Computes initial next_run_at from cron expression.
pub async fn create_schedule(
    pool: &PgPool,
    user_id: Uuid,
    agent_id: &str,
    name: &str,
    description: Option<&str>,
    cron_expression: &str,
    natural_language: Option<&str>,
    task_prompt: &str,
    delivery_channel: Option<&str>,
    delivery_target: Option<&str>,
) -> anyhow::Result<ScheduleInfo> {
    // Validate cron expression
    let next = next_run_after(cron_expression, Utc::now())
        .ok_or_else(|| anyhow::anyhow!("Invalid cron expression: {}", cron_expression))?;

    let row = sqlx::query_as::<_, ScheduleInfo>(
        "INSERT INTO schedules (user_id, agent_id, name, description, cron_expression, \
         natural_language, task_prompt, delivery_channel, delivery_target, next_run_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         RETURNING id, user_id, agent_id, name, description, cron_expression, natural_language, \
         task_prompt, delivery_channel, delivery_target, is_active, last_run_at, next_run_at, \
         run_count, error_count, last_error, created_at",
    )
    .bind(user_id)
    .bind(agent_id)
    .bind(name)
    .bind(description)
    .bind(cron_expression)
    .bind(natural_language)
    .bind(task_prompt)
    .bind(delivery_channel)
    .bind(delivery_target)
    .bind(next)
    .fetch_one(pool)
    .await?;

    info!(schedule_id = %row.id, name = name, next_run = %next, "Schedule created");
    Ok(row)
}

/// Update an existing schedule.
pub async fn update_schedule(
    pool: &PgPool,
    id: Uuid,
    name: Option<&str>,
    description: Option<&str>,
    cron_expression: Option<&str>,
    natural_language: Option<&str>,
    task_prompt: Option<&str>,
    delivery_channel: Option<&str>,
    delivery_target: Option<&str>,
    is_active: Option<bool>,
) -> anyhow::Result<Option<ScheduleInfo>> {
    // If cron expression is changing, validate and compute new next_run_at
    let new_next: Option<DateTime<Utc>> = if let Some(cron) = cron_expression {
        Some(
            next_run_after(cron, Utc::now())
                .ok_or_else(|| anyhow::anyhow!("Invalid cron expression: {}", cron))?,
        )
    } else {
        None
    };

    // Build dynamic update — use COALESCE pattern to only update provided fields
    let row = sqlx::query_as::<_, ScheduleInfo>(
        "UPDATE schedules SET \
         name = COALESCE($2, name), \
         description = COALESCE($3, description), \
         cron_expression = COALESCE($4, cron_expression), \
         natural_language = COALESCE($5, natural_language), \
         task_prompt = COALESCE($6, task_prompt), \
         delivery_channel = COALESCE($7, delivery_channel), \
         delivery_target = COALESCE($8, delivery_target), \
         is_active = COALESCE($9, is_active), \
         next_run_at = COALESCE($10, next_run_at) \
         WHERE id = $1 \
         RETURNING id, user_id, agent_id, name, description, cron_expression, natural_language, \
         task_prompt, delivery_channel, delivery_target, is_active, last_run_at, next_run_at, \
         run_count, error_count, last_error, created_at",
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .bind(cron_expression)
    .bind(natural_language)
    .bind(task_prompt)
    .bind(delivery_channel)
    .bind(delivery_target)
    .bind(is_active)
    .bind(new_next)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Delete a schedule by ID.
pub async fn delete_schedule(pool: &PgPool, id: Uuid) -> anyhow::Result<bool> {
    // Delete runs first (FK constraint)
    sqlx::query("DELETE FROM schedule_runs WHERE schedule_id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    let result = sqlx::query("DELETE FROM schedules WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// List run history for a schedule.
pub async fn list_runs(
    pool: &PgPool,
    schedule_id: Uuid,
    limit: i64,
) -> anyhow::Result<Vec<ScheduleRunInfo>> {
    let rows = sqlx::query_as::<_, ScheduleRunInfo>(
        "SELECT id, schedule_id, started_at, completed_at, status, result_summary, \
         conversation_id, tokens_used, cost_usd::float8, error \
         FROM schedule_runs WHERE schedule_id = $1 ORDER BY started_at DESC LIMIT $2",
    )
    .bind(schedule_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
