use axum::{extract::{Query, State}, routing::get, Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(search_audit))
}

#[derive(Deserialize)]
struct AuditQuery {
    user_id: Option<Uuid>,
    event_type: Option<String>,
    conversation_id: Option<Uuid>,
    from: Option<String>,
    to: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

async fn search_audit(
    State(state): State<AppState>,
    Query(params): Query<AuditQuery>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let from_ts = params
        .from
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let to_ts = params
        .to
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let filter = velkor_audit::logger::AuditFilter {
        user_id: params.user_id,
        event_type: params.event_type.clone(),
        conversation_id: params.conversation_id,
        request_id: None,
        from: from_ts,
        to: to_ts,
    };

    let entries = state
        .audit
        .search(&filter, params.limit as usize, params.offset as usize)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let json_entries: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "timestamp": e.timestamp.to_rfc3339(),
                "event_type": e.event_type,
                "user_id": e.user_id,
                "agent_id": e.agent_id,
                "conversation_id": e.conversation_id,
                "details": e.details,
                "model_used": e.model_used,
                "tokens_input": e.tokens_input,
                "tokens_output": e.tokens_output,
                "cost_usd": e.cost_usd,
                "request_id": e.request_id,
            })
        })
        .collect();

    Ok(Json(serde_json::json!(json_entries)))
}
