use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tasks).post(spawn_task))
        .route("/{id}", get(get_task))
        .route("/{id}/cancel", axum::routing::post(cancel_task))
        .route("/agents", get(list_agents))
        .route("/notifications", get(task_notifications_sse))
}

// ---------------------------------------------------------------------------
// List tasks
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ListParams {
    user_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list_tasks(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<velkor_orchestrator::tasks::BackgroundTask>>, (StatusCode, String)> {
    velkor_orchestrator::tasks::list_tasks(&state.pool, params.user_id, params.limit)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

// ---------------------------------------------------------------------------
// Get task
// ---------------------------------------------------------------------------

async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<velkor_orchestrator::tasks::BackgroundTask>, (StatusCode, String)> {
    velkor_orchestrator::tasks::get_task(&state.pool, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "Task not found".to_string()))
}

// ---------------------------------------------------------------------------
// Spawn task
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SpawnTask {
    user_id: Uuid,
    agent_id: Option<String>,
    title: String,
    task_prompt: String,
    source_conversation_id: Option<Uuid>,
}

async fn spawn_task(
    State(state): State<AppState>,
    Json(body): Json<SpawnTask>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let agent_id = body.agent_id.as_deref().unwrap_or("default");

    // Get the runtime for this agent (fall back to default)
    let runtime = if let Some(orch) = &state.orchestrator {
        orch.get_agent(agent_id)
            .or_else(|| orch.get_agent("default"))
            .cloned()
            .unwrap_or_else(|| state.runtime.clone())
    } else {
        state.runtime.clone()
    };

    let task_id = velkor_orchestrator::tasks::spawn_task(
        &state.pool,
        runtime,
        state.task_notifier.clone(),
        body.user_id,
        agent_id,
        &body.title,
        &body.task_prompt,
        body.source_conversation_id,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "task_id": task_id, "status": "pending" })),
    ))
}

// ---------------------------------------------------------------------------
// Cancel task
// ---------------------------------------------------------------------------

async fn cancel_task(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let cancelled = velkor_orchestrator::tasks::cancel_task(&state.pool, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if cancelled {
        Ok(StatusCode::OK)
    } else {
        Err((StatusCode::NOT_FOUND, "Task not found or already finished".to_string()))
    }
}

// ---------------------------------------------------------------------------
// Task notifications SSE
// ---------------------------------------------------------------------------

/// SSE endpoint that streams task completion notifications.
/// The gateway subscribes on behalf of connected WebSocket users.
/// Query param: user_id (optional, filters to only that user's tasks)
#[derive(Deserialize)]
struct NotificationParams {
    user_id: Option<Uuid>,
}

async fn task_notifications_sse(
    State(state): State<AppState>,
    Query(params): Query<NotificationParams>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut rx = state.task_notifier.subscribe();
    let user_filter = params.user_id;

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(notification) => {
                    // Filter by user if specified
                    if let Some(uid) = user_filter {
                        if notification.user_id != uid {
                            continue;
                        }
                    }
                    if let Ok(json) = serde_json::to_string(&notification) {
                        yield Ok(Event::default().event("task_complete").data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(lagged = n, "Task notification subscriber lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// List agents
// ---------------------------------------------------------------------------

async fn list_agents(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    if let Some(ref orch) = state.orchestrator {
        Json(serde_json::json!({
            "agents": orch.list_agents(),
            "supervisor": orch.supervisor_id(),
        }))
    } else {
        Json(serde_json::json!({
            "agents": [{
                "id": "default",
                "model": state.runtime.config.model,
                "is_supervisor": true,
            }],
            "supervisor": "default",
        }))
    }
}
