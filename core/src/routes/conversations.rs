use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_conversations))
        .route("/:id", get(get_conversation).delete(delete_conversation))
}

#[derive(Deserialize)]
struct ListParams {
    user_id: Uuid,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list_conversations(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let rows = sqlx::query_as::<_, ConvRow>(
        r#"
        SELECT id, title, summary, started_at, ended_at
        FROM conversations
        WHERE user_id = $1 AND NOT is_deleted
        ORDER BY started_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(params.user_id)
    .bind(params.limit)
    .bind(params.offset)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let conversations: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "title": r.title,
                "summary": r.summary,
                "started_at": r.started_at.to_rfc3339(),
                "ended_at": r.ended_at.map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(serde_json::json!(conversations)))
}

async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let conv = sqlx::query_as::<_, ConvRow>(
        "SELECT id, title, summary, started_at, ended_at FROM conversations WHERE id = $1 AND NOT is_deleted",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    let messages = sqlx::query_as::<_, MsgRow>(
        "SELECT role, content, created_at FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let msgs: Vec<serde_json::Value> = messages
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
                "created_at": m.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "id": conv.id,
        "title": conv.title,
        "summary": conv.summary,
        "started_at": conv.started_at.to_rfc3339(),
        "ended_at": conv.ended_at.map(|t| t.to_rfc3339()),
        "messages": msgs,
    })))
}

async fn delete_conversation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    sqlx::query("UPDATE conversations SET is_deleted = TRUE, ended_at = now() WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

#[derive(sqlx::FromRow)]
struct ConvRow {
    id: Uuid,
    title: Option<String>,
    summary: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    ended_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(sqlx::FromRow)]
struct MsgRow {
    role: String,
    content: String,
    created_at: chrono::DateTime<chrono::Utc>,
}
