//! Background tasks — long-running agent work spawned on demand.
//!
//! Users kick off a task and continue chatting. The task runs in a background
//! tokio task, persists results to the `background_tasks` table, and notifies
//! via a callback when complete.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};
use uuid::Uuid;
use velkor_runtime::context::ConversationContext;
use velkor_runtime::react::AgentRuntime;

// ---------------------------------------------------------------------------
// Task notification channel
// ---------------------------------------------------------------------------

/// Event emitted when a background task completes (or fails).
/// Listeners (e.g. WebSocket handlers) subscribe to receive these.
#[derive(Debug, Clone, Serialize)]
pub struct TaskNotification {
    pub task_id: Uuid,
    pub user_id: Uuid,
    pub status: String, // "completed" or "failed"
    pub title: String,
    pub result_summary: Option<String>,
    pub conversation_id: Option<Uuid>,
    pub error: Option<String>,
    pub tokens_used: i32,
}

/// Shared broadcast channel for task notifications.
pub type TaskNotifier = Arc<broadcast::Sender<TaskNotification>>;

/// Create a new task notifier (broadcast channel).
pub fn new_notifier() -> TaskNotifier {
    let (tx, _) = broadcast::channel(64);
    Arc::new(tx)
}

// ---------------------------------------------------------------------------
// Task DB types
// ---------------------------------------------------------------------------

/// Row from the background_tasks table.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BackgroundTask {
    pub id: Uuid,
    pub user_id: Uuid,
    pub agent_id: String,
    pub title: String,
    pub task_prompt: String,
    pub status: String,
    pub result_summary: Option<String>,
    pub conversation_id: Option<Uuid>,
    pub source_conversation_id: Option<Uuid>,
    pub tokens_used: Option<i32>,
    pub cost_usd: Option<f64>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Spawn a background task
// ---------------------------------------------------------------------------

/// Spawn a new background task. Creates a DB record, then runs the agent
/// in a background tokio task. Returns the task ID immediately.
pub async fn spawn_task(
    pool: &PgPool,
    runtime: Arc<AgentRuntime>,
    notifier: TaskNotifier,
    user_id: Uuid,
    agent_id: &str,
    title: &str,
    task_prompt: &str,
    source_conversation_id: Option<Uuid>,
) -> anyhow::Result<Uuid> {
    let task_id = Uuid::new_v4();
    let conversation_id = Uuid::new_v4();

    // Insert the task record as 'pending'
    sqlx::query(
        "INSERT INTO background_tasks \
         (id, user_id, agent_id, title, task_prompt, status, conversation_id, source_conversation_id) \
         VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)",
    )
    .bind(task_id)
    .bind(user_id)
    .bind(agent_id)
    .bind(title)
    .bind(task_prompt)
    .bind(conversation_id)
    .bind(source_conversation_id)
    .execute(pool)
    .await?;

    info!(
        task_id = %task_id,
        agent_id = agent_id,
        title = title,
        "Background task created"
    );

    // Spawn the actual work
    let pool2 = pool.clone();
    let agent_id2 = agent_id.to_string();
    let title2 = title.to_string();
    let task_prompt2 = task_prompt.to_string();

    tokio::spawn(async move {
        execute_task(
            &pool2,
            &runtime,
            notifier,
            task_id,
            conversation_id,
            user_id,
            &agent_id2,
            &title2,
            &task_prompt2,
        )
        .await;
    });

    Ok(task_id)
}

/// Internal: execute the background task.
async fn execute_task(
    pool: &PgPool,
    runtime: &AgentRuntime,
    notifier: TaskNotifier,
    task_id: Uuid,
    conversation_id: Uuid,
    user_id: Uuid,
    agent_id: &str,
    title: &str,
    task_prompt: &str,
) {
    // Mark as running
    let _ = sqlx::query(
        "UPDATE background_tasks SET status = 'running', started_at = now() WHERE id = $1",
    )
    .bind(task_id)
    .execute(pool)
    .await;

    // Create conversation record
    let _ = sqlx::query(
        "INSERT INTO conversations (id, user_id, agent_id, title, started_at) \
         VALUES ($1, $2, $3, $4, now())",
    )
    .bind(conversation_id)
    .bind(user_id)
    .bind(agent_id)
    .bind(format!("[Task] {}", title))
    .execute(pool)
    .await;

    // Persist user message
    let _ = sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) \
         VALUES ($1, $2, 'user', $3, now())",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(task_prompt)
    .execute(pool)
    .await;

    // Run the agent
    let mut context = ConversationContext::new(conversation_id, user_id, agent_id);
    let result = runtime.run(task_prompt, &mut context).await;

    match result {
        Ok(response) => {
            let total_tokens =
                (response.usage.input_tokens + response.usage.output_tokens) as i32;
            let summary = if response.content.len() > 1000 {
                format!("{}...", &response.content[..1000])
            } else {
                response.content.clone()
            };

            // Persist assistant response
            let _ = sqlx::query(
                "INSERT INTO messages (id, conversation_id, role, content, created_at) \
                 VALUES ($1, $2, 'assistant', $3, now())",
            )
            .bind(Uuid::new_v4())
            .bind(conversation_id)
            .bind(&response.content)
            .execute(pool)
            .await;

            // Update task as completed
            let _ = sqlx::query(
                "UPDATE background_tasks SET \
                 status = 'completed', completed_at = now(), \
                 result_summary = $1, tokens_used = $2 \
                 WHERE id = $3",
            )
            .bind(&summary)
            .bind(total_tokens)
            .bind(task_id)
            .execute(pool)
            .await;

            info!(
                task_id = %task_id,
                tokens = total_tokens,
                iterations = response.iterations,
                "Background task completed"
            );

            // Notify listeners
            let _ = notifier.send(TaskNotification {
                task_id,
                user_id,
                status: "completed".to_string(),
                title: title.to_string(),
                result_summary: Some(summary),
                conversation_id: Some(conversation_id),
                error: None,
                tokens_used: total_tokens,
            });
        }
        Err(e) => {
            let err_msg = format!("{e}");

            let _ = sqlx::query(
                "UPDATE background_tasks SET \
                 status = 'failed', completed_at = now(), error = $1 \
                 WHERE id = $2",
            )
            .bind(&err_msg)
            .bind(task_id)
            .execute(pool)
            .await;

            error!(task_id = %task_id, error = %e, "Background task failed");

            let _ = notifier.send(TaskNotification {
                task_id,
                user_id,
                status: "failed".to_string(),
                title: title.to_string(),
                result_summary: None,
                conversation_id: Some(conversation_id),
                error: Some(err_msg),
                tokens_used: 0,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// CRUD helpers
// ---------------------------------------------------------------------------

/// List background tasks for a user (or all if None).
pub async fn list_tasks(
    pool: &PgPool,
    user_id: Option<Uuid>,
    limit: i64,
) -> anyhow::Result<Vec<BackgroundTask>> {
    let rows = if let Some(uid) = user_id {
        sqlx::query_as::<_, BackgroundTask>(
            "SELECT id, user_id, agent_id, title, task_prompt, status, result_summary, \
             conversation_id, source_conversation_id, tokens_used, cost_usd::float8, error, \
             created_at, started_at, completed_at \
             FROM background_tasks WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(uid)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, BackgroundTask>(
            "SELECT id, user_id, agent_id, title, task_prompt, status, result_summary, \
             conversation_id, source_conversation_id, tokens_used, cost_usd::float8, error, \
             created_at, started_at, completed_at \
             FROM background_tasks ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

/// Get a single task by ID.
pub async fn get_task(pool: &PgPool, id: Uuid) -> anyhow::Result<Option<BackgroundTask>> {
    let row = sqlx::query_as::<_, BackgroundTask>(
        "SELECT id, user_id, agent_id, title, task_prompt, status, result_summary, \
         conversation_id, source_conversation_id, tokens_used, cost_usd::float8, error, \
         created_at, started_at, completed_at \
         FROM background_tasks WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Cancel a pending or running task (sets status to 'cancelled').
/// Note: this doesn't actually abort the tokio task — it just marks it.
/// The agent will finish its current iteration but results won't be used.
pub async fn cancel_task(pool: &PgPool, id: Uuid) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE background_tasks SET status = 'cancelled', completed_at = now() \
         WHERE id = $1 AND status IN ('pending', 'running')",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
