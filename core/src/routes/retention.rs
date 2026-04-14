use axum::{extract::State, routing::get, Json, Router};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/status", get(retention_status))
}

async fn retention_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let status = state.pulse_status.read().await;
    // Extract the retention subsystem status from the unified pulse status
    let retention = status
        .subsystems
        .iter()
        .find(|s| s.name == "retention");
    Json(serde_json::json!({
        "running": status.running,
        "subsystem": retention,
    }))
}
