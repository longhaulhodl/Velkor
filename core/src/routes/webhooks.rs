//! Public webhook intake: `POST /webhooks/:trigger_id`.
//!
//! - Looks up the trigger (must exist, must be kind='webhook' and active).
//! - If `config.verify == true` (or implied by a configured secret), reads the
//!   signature header (default `X-Velkor-Signature`, overridable via
//!   `config.signature_header`) and HMAC-verifies the raw body.
//! - Enqueues a row into `trigger_events` and returns 202 Accepted.
//!
//! Notes:
//! - This endpoint is NOT under `/internal/` — it's meant to be reachable by
//!   external systems (GitHub, Stripe, etc). In production, front with the
//!   gateway / a reverse proxy + TLS.
//! - We verify against the raw body bytes (not parsed JSON) so signatures match.

use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use std::net::SocketAddr;
use uuid::Uuid;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/{trigger_id}", post(receive_webhook))
}

async fn receive_webhook(
    State(state): State<AppState>,
    Path(trigger_id): Path<Uuid>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let trigger = velkor_triggers::get_trigger(&state.pool, trigger_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Trigger not found".to_string()))?;

    if trigger.kind != "webhook" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Trigger {trigger_id} is not a webhook (kind={})", trigger.kind),
        ));
    }
    if !trigger.is_active {
        return Err((StatusCode::GONE, "Trigger is inactive".to_string()));
    }

    // Read config
    let cfg = &trigger.config;
    let secret = cfg
        .get("secret")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let has_secret = !secret.is_empty();
    let verify = cfg
        .get("verify")
        .and_then(|v| v.as_bool())
        .unwrap_or(has_secret);
    let sig_header_name = cfg
        .get("signature_header")
        .and_then(|v| v.as_str())
        .unwrap_or("X-Velkor-Signature");

    if verify {
        let sig = headers
            .get(sig_header_name)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    format!("Missing signature header: {sig_header_name}"),
                )
            })?;

        velkor_triggers::verify_signature(&body, sig, secret, None)
            .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Signature verification failed: {e}")))?;
    }

    // Parse body as JSON if possible, otherwise store as a string under "raw"
    let payload: serde_json::Value = serde_json::from_slice(&body)
        .unwrap_or_else(|_| serde_json::json!({ "raw": String::from_utf8_lossy(&body) }));

    let source_ip = Some(addr.ip().to_string());

    let event_id =
        velkor_triggers::enqueue_webhook_event(&state.pool, trigger_id, payload, source_ip)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "event_id": event_id, "status": "queued" })),
    ))
}
