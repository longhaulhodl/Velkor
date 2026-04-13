use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;
use velkor_memory::{MemoryCategory, MemoryScope};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(store_memory))
        .route("/search", post(search_memory))
        .route("/{id}", get(get_memory).put(update_memory).delete(delete_memory))
}

#[derive(Deserialize)]
struct StoreRequest {
    user_id: Uuid,
    content: String,
    scope: String,
    category: Option<String>,
    source_conversation_id: Option<Uuid>,
}

async fn store_memory(
    State(state): State<AppState>,
    Json(req): Json<StoreRequest>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let scope = parse_scope(&req.scope);
    let category = req.category.as_deref().and_then(parse_category);

    let id = state
        .memory
        .store(
            req.user_id,
            &req.content,
            scope,
            category,
            req.source_conversation_id,
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "id": id })))
}

#[derive(Deserialize)]
struct SearchRequest {
    user_id: Uuid,
    query: String,
    scope: String,
    limit: usize,
}

async fn search_memory(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let scope = parse_scope(&req.scope);

    let results = state
        .memory
        .search(&req.query, scope, req.user_id, req.limit)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let json_results: Vec<serde_json::Value> = results
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "content": r.content,
                "scope": r.scope.as_str(),
                "category": r.category.map(|c| c.as_str().to_string()),
                "confidence": r.confidence,
                "score": r.score,
                "created_at": r.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!(json_results)))
}

async fn get_memory(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let record = state
        .memory
        .get(id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "id": record.id,
        "content": record.content,
        "scope": record.scope.as_str(),
        "category": record.category.map(|c| c.as_str().to_string()),
        "confidence": record.confidence,
        "created_at": record.created_at.to_rfc3339(),
        "updated_at": record.updated_at.to_rfc3339(),
    })))
}

#[derive(Deserialize)]
struct UpdateRequest {
    content: String,
}

async fn update_memory(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRequest>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    state
        .memory
        .update(id, &req.content)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "updated": true })))
}

async fn delete_memory(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    state
        .memory
        .delete(id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

fn parse_scope(s: &str) -> MemoryScope {
    match s {
        "shared" => MemoryScope::Shared,
        "org" => MemoryScope::Org,
        _ => MemoryScope::Personal,
    }
}

fn parse_category(s: &str) -> Option<MemoryCategory> {
    match s {
        "fact" => Some(MemoryCategory::Fact),
        "preference" => Some(MemoryCategory::Preference),
        "project" => Some(MemoryCategory::Project),
        "procedure" => Some(MemoryCategory::Procedure),
        "relationship" => Some(MemoryCategory::Relationship),
        _ => None,
    }
}
