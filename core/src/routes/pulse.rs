use axum::{extract::State, routing::get, Json, Router};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/status", get(pulse_status))
}

async fn pulse_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state.pulse_status.read().await;
    Json(serde_json::json!(&*status))
}
