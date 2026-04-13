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
        .route("/", get(list_documents))
        .route("/{id}", get(get_document).delete(delete_document))
}

#[derive(Deserialize)]
struct ListParams {
    workspace_id: Uuid,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list_documents(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let rows = sqlx::query_as::<_, DocRow>(
        r#"
        SELECT id, workspace_id, filename, mime_type, file_size, created_at
        FROM documents
        WHERE workspace_id = $1 AND NOT is_deleted
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(params.workspace_id)
    .bind(params.limit)
    .bind(params.offset)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let docs: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "workspace_id": r.workspace_id,
                "filename": r.filename,
                "mime_type": r.mime_type,
                "file_size": r.file_size,
                "created_at": r.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!(docs)))
}

async fn get_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let row = sqlx::query_as::<_, DocRow>(
        "SELECT id, workspace_id, filename, mime_type, file_size, created_at FROM documents WHERE id = $1 AND NOT is_deleted",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "filename": row.filename,
        "mime_type": row.mime_type,
        "file_size": row.file_size,
        "created_at": row.created_at.to_rfc3339(),
    })))
}

async fn delete_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    // Check legal hold
    let hold = sqlx::query_scalar::<_, bool>(
        "SELECT legal_hold FROM documents WHERE id = $1 AND NOT is_deleted",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    match hold {
        None => return Err(axum::http::StatusCode::NOT_FOUND),
        Some(true) => return Err(axum::http::StatusCode::FORBIDDEN),
        Some(false) => {}
    }

    sqlx::query("UPDATE documents SET is_deleted = TRUE, deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

#[derive(sqlx::FromRow)]
struct DocRow {
    id: Uuid,
    workspace_id: Uuid,
    filename: String,
    mime_type: Option<String>,
    file_size: Option<i64>,
    created_at: chrono::DateTime<chrono::Utc>,
}
