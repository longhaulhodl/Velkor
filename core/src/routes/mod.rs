pub mod chat;
pub mod conversations;
pub mod memory;
pub mod documents;
pub mod audit;
pub mod retention;
pub mod users;

use axum::{routing::get, Json, Router};
use crate::AppState;

/// Build the internal API router that the TypeScript gateway calls.
pub fn internal_router() -> Router<AppState> {
    Router::new()
        .route("/internal/health", get(health))
        .nest("/internal/chat", chat::router())
        .nest("/internal/conversations", conversations::router())
        .nest("/internal/memory", memory::router())
        .nest("/internal/documents", documents::router())
        .nest("/internal/audit", audit::router())
        .nest("/internal/retention", retention::router())
        .nest("/internal/users", users::router())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "velkor-core" }))
}
