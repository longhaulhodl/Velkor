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
        .route("/", get(list_schedules).post(create_schedule))
        .route("/{id}", get(get_schedule).put(update_schedule).delete(delete_schedule))
        .route("/{id}/runs", get(list_runs))
        .route("/status", get(scheduler_status))
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

async fn scheduler_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state.scheduler_status.read().await;
    Json(serde_json::json!(&*status))
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ListParams {
    user_id: Option<Uuid>,
}

async fn list_schedules(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<velkor_scheduler::ScheduleInfo>>, (StatusCode, String)> {
    velkor_scheduler::list_schedules(&state.pool, params.user_id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

// ---------------------------------------------------------------------------
// Get
// ---------------------------------------------------------------------------

async fn get_schedule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<velkor_scheduler::ScheduleInfo>, (StatusCode, String)> {
    velkor_scheduler::get_schedule(&state.pool, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "Schedule not found".to_string()))
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateSchedule {
    user_id: Uuid,
    agent_id: Option<String>,
    name: String,
    description: Option<String>,
    cron_expression: String,
    natural_language: Option<String>,
    task_prompt: String,
    delivery_channel: Option<String>,
    delivery_target: Option<String>,
}

async fn create_schedule(
    State(state): State<AppState>,
    Json(body): Json<CreateSchedule>,
) -> Result<(StatusCode, Json<velkor_scheduler::ScheduleInfo>), (StatusCode, String)> {
    velkor_scheduler::create_schedule(
        &state.pool,
        body.user_id,
        body.agent_id.as_deref().unwrap_or("default"),
        &body.name,
        body.description.as_deref(),
        &body.cron_expression,
        body.natural_language.as_deref(),
        &body.task_prompt,
        body.delivery_channel.as_deref(),
        body.delivery_target.as_deref(),
    )
    .await
    .map(|s| (StatusCode::CREATED, Json(s)))
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct UpdateSchedule {
    name: Option<String>,
    description: Option<String>,
    cron_expression: Option<String>,
    natural_language: Option<String>,
    task_prompt: Option<String>,
    delivery_channel: Option<String>,
    delivery_target: Option<String>,
    is_active: Option<bool>,
}

async fn update_schedule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateSchedule>,
) -> Result<Json<velkor_scheduler::ScheduleInfo>, (StatusCode, String)> {
    velkor_scheduler::update_schedule(
        &state.pool,
        id,
        body.name.as_deref(),
        body.description.as_deref(),
        body.cron_expression.as_deref(),
        body.natural_language.as_deref(),
        body.task_prompt.as_deref(),
        body.delivery_channel.as_deref(),
        body.delivery_target.as_deref(),
        body.is_active,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .ok_or((StatusCode::NOT_FOUND, "Schedule not found".to_string()))
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

async fn delete_schedule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let deleted = velkor_scheduler::delete_schedule(&state.pool, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "Schedule not found".to_string()))
    }
}

// ---------------------------------------------------------------------------
// Run history
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RunsParams {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list_runs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<RunsParams>,
) -> Result<Json<Vec<velkor_scheduler::ScheduleRunInfo>>, (StatusCode, String)> {
    velkor_scheduler::list_runs(&state.pool, id, params.limit)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
