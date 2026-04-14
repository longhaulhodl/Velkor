use axum::{extract::State, routing::get, Json, Router};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/status", get(retention_status))
}

async fn retention_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let status = state.retention_status.read().await;
    Json(serde_json::json!(&*status))
}
