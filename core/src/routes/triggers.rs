//! Internal CRUD routes for triggers. Mounted at /internal/triggers.
//!
//! Public webhook intake lives in routes/webhooks.rs (no `/internal/` prefix).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_triggers).post(create_trigger))
        .route("/{id}", get(get_trigger).put(update_trigger).delete(delete_trigger))
        .route("/{id}/events", get(list_events))
}

#[derive(Deserialize)]
struct ListParams {
    user_id: Option<Uuid>,
}

async fn list_triggers(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<velkor_triggers::TriggerInfo>>, (StatusCode, String)> {
    velkor_triggers::list_triggers(&state.pool, params.user_id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn get_trigger(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<velkor_triggers::TriggerInfo>, (StatusCode, String)> {
    velkor_triggers::get_trigger(&state.pool, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "Trigger not found".to_string()))
}

#[derive(Deserialize)]
struct CreateTrigger {
    user_id: Uuid,
    name: String,
    description: Option<String>,
    kind: String,
    config: Option<serde_json::Value>,
    agent_id: Option<String>,
    prompt_template: String,
}

async fn create_trigger(
    State(state): State<AppState>,
    Json(body): Json<CreateTrigger>,
) -> Result<(StatusCode, Json<velkor_triggers::TriggerInfo>), (StatusCode, String)> {
    velkor_triggers::create_trigger(
        &state.pool,
        body.user_id,
        &body.name,
        body.description.as_deref(),
        &body.kind,
        body.config.unwrap_or_else(|| serde_json::json!({})),
        body.agent_id.as_deref().unwrap_or("default"),
        &body.prompt_template,
    )
    .await
    .map(|t| (StatusCode::CREATED, Json(t)))
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

#[derive(Deserialize)]
struct UpdateTrigger {
    name: Option<String>,
    description: Option<String>,
    config: Option<serde_json::Value>,
    agent_id: Option<String>,
    prompt_template: Option<String>,
    is_active: Option<bool>,
}

async fn update_trigger(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTrigger>,
) -> Result<Json<velkor_triggers::TriggerInfo>, (StatusCode, String)> {
    velkor_triggers::update_trigger(
        &state.pool,
        id,
        body.name.as_deref(),
        body.description.as_deref(),
        body.config,
        body.agent_id.as_deref(),
        body.prompt_template.as_deref(),
        body.is_active,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .ok_or((StatusCode::NOT_FOUND, "Trigger not found".to_string()))
}

async fn delete_trigger(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let deleted = velkor_triggers::delete_trigger(&state.pool, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "Trigger not found".to_string()))
    }
}

#[derive(Deserialize)]
struct EventsParams {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<EventsParams>,
) -> Result<Json<Vec<velkor_triggers::TriggerEventInfo>>, (StatusCode, String)> {
    velkor_triggers::list_events(&state.pool, id, params.limit)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
