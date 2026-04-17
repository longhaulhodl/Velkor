//! CRUD for triggers + event listing. Pure DB helpers, called from route handlers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TriggerInfo {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub kind: String,
    pub config: serde_json::Value,
    pub agent_id: String,
    pub prompt_template: String,
    pub is_active: bool,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub fire_count: i32,
    pub error_count: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TriggerEventInfo {
    pub id: Uuid,
    pub trigger_id: Uuid,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempts: i32,
    pub error: Option<String>,
    pub conversation_id: Option<Uuid>,
    pub source_ip: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

const TRIGGER_COLS: &str = "id, user_id, name, description, kind, config, agent_id, \
    prompt_template, is_active, last_fired_at, fire_count, error_count, last_error, created_at";

/// List all triggers, optionally scoped to a user.
pub async fn list_triggers(
    pool: &PgPool,
    user_id: Option<Uuid>,
) -> anyhow::Result<Vec<TriggerInfo>> {
    let rows = if let Some(uid) = user_id {
        sqlx::query_as::<_, TriggerInfo>(&format!(
            "SELECT {TRIGGER_COLS} FROM triggers WHERE user_id = $1 ORDER BY created_at DESC"
        ))
        .bind(uid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, TriggerInfo>(&format!(
            "SELECT {TRIGGER_COLS} FROM triggers ORDER BY created_at DESC"
        ))
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

pub async fn get_trigger(pool: &PgPool, id: Uuid) -> anyhow::Result<Option<TriggerInfo>> {
    let row = sqlx::query_as::<_, TriggerInfo>(&format!(
        "SELECT {TRIGGER_COLS} FROM triggers WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

fn validate_kind(kind: &str) -> anyhow::Result<()> {
    match kind {
        "webhook" | "file_watch" | "email" => Ok(()),
        other => Err(anyhow::anyhow!(
            "Invalid trigger kind: {other} (expected webhook|file_watch|email)"
        )),
    }
}

/// Create a new trigger.
#[allow(clippy::too_many_arguments)]
pub async fn create_trigger(
    pool: &PgPool,
    user_id: Uuid,
    name: &str,
    description: Option<&str>,
    kind: &str,
    config: serde_json::Value,
    agent_id: &str,
    prompt_template: &str,
) -> anyhow::Result<TriggerInfo> {
    validate_kind(kind)?;

    // Minimal kind-specific validation
    if kind == "file_watch" {
        let path = config.get("path").and_then(|v| v.as_str());
        if path.is_none() {
            anyhow::bail!("file_watch trigger requires config.path");
        }
    }
    if kind == "webhook" {
        // A secret is not strictly required (verify=false allowed for internal use),
        // but we nudge towards it — verify defaults to true below if a secret is present.
        let has_secret = config
            .get("secret")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let verify = config
            .get("verify")
            .and_then(|v| v.as_bool())
            .unwrap_or(has_secret);
        if verify && !has_secret {
            anyhow::bail!("webhook trigger with verify=true requires config.secret");
        }
    }

    let row = sqlx::query_as::<_, TriggerInfo>(&format!(
        "INSERT INTO triggers (user_id, name, description, kind, config, agent_id, prompt_template) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {TRIGGER_COLS}"
    ))
    .bind(user_id)
    .bind(name)
    .bind(description)
    .bind(kind)
    .bind(config)
    .bind(agent_id)
    .bind(prompt_template)
    .fetch_one(pool)
    .await?;

    info!(trigger_id = %row.id, kind = kind, name = name, "Trigger created");
    Ok(row)
}

/// Partial update — any `Some` field is patched, others retained via COALESCE.
#[allow(clippy::too_many_arguments)]
pub async fn update_trigger(
    pool: &PgPool,
    id: Uuid,
    name: Option<&str>,
    description: Option<&str>,
    config: Option<serde_json::Value>,
    agent_id: Option<&str>,
    prompt_template: Option<&str>,
    is_active: Option<bool>,
) -> anyhow::Result<Option<TriggerInfo>> {
    let row = sqlx::query_as::<_, TriggerInfo>(&format!(
        "UPDATE triggers SET \
         name = COALESCE($2, name), \
         description = COALESCE($3, description), \
         config = COALESCE($4, config), \
         agent_id = COALESCE($5, agent_id), \
         prompt_template = COALESCE($6, prompt_template), \
         is_active = COALESCE($7, is_active), \
         updated_at = now() \
         WHERE id = $1 RETURNING {TRIGGER_COLS}"
    ))
    .bind(id)
    .bind(name)
    .bind(description)
    .bind(config)
    .bind(agent_id)
    .bind(prompt_template)
    .bind(is_active)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn delete_trigger(pool: &PgPool, id: Uuid) -> anyhow::Result<bool> {
    // trigger_events has ON DELETE CASCADE, so this single query is enough.
    let result = sqlx::query("DELETE FROM triggers WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// List recent events for a trigger (newest first).
pub async fn list_events(
    pool: &PgPool,
    trigger_id: Uuid,
    limit: i64,
) -> anyhow::Result<Vec<TriggerEventInfo>> {
    let rows = sqlx::query_as::<_, TriggerEventInfo>(
        "SELECT id, trigger_id, payload, status, attempts, error, conversation_id, \
         source_ip, created_at, started_at, completed_at \
         FROM trigger_events WHERE trigger_id = $1 ORDER BY created_at DESC LIMIT $2",
    )
    .bind(trigger_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
