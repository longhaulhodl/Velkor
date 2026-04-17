//! Webhook ingestion: HMAC signature verification + event enqueuing.
//!
//! PRD Section 20.1 specifies the signature format `t=<unix_ts>,v1=<hex_hmac>`
//! and HMAC-SHA256 over `"<timestamp>.<payload>"`. We accept that format and
//! the simpler GitHub-style `sha256=<hex>` header as a fallback.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Error)]
pub enum WebhookVerifyError {
    #[error("missing signature header")]
    MissingSignature,
    #[error("malformed signature header")]
    MalformedSignature,
    #[error("signature timestamp outside tolerance window")]
    TimestampOutOfRange,
    #[error("signature mismatch")]
    SignatureMismatch,
    #[error("secret not configured but verify=true")]
    MissingSecret,
}

/// Default signature tolerance window (seconds). Prevents replay attacks.
const DEFAULT_TOLERANCE_SECS: i64 = 300;

/// Verify a webhook signature. Accepts two formats:
///
/// 1. Velkor/Stripe-style: `t=1700000000,v1=abc123...`  (HMAC over `"t.payload"`)
/// 2. GitHub-style:        `sha256=abc123...`           (HMAC over raw payload)
///
/// `tolerance_secs = None` uses the default 300s (5 min). For format 2 there is
/// no timestamp so tolerance is not checked.
pub fn verify_signature(
    payload: &[u8],
    signature: &str,
    secret: &str,
    tolerance_secs: Option<i64>,
) -> Result<(), WebhookVerifyError> {
    if secret.is_empty() {
        return Err(WebhookVerifyError::MissingSecret);
    }
    let sig = signature.trim();
    if sig.is_empty() {
        return Err(WebhookVerifyError::MissingSignature);
    }

    // Format 2: GitHub-style "sha256=<hex>"
    if let Some(hex_sig) = sig.strip_prefix("sha256=") {
        return verify_hex(payload, hex_sig, secret);
    }

    // Format 1: "t=<ts>,v1=<hex>"
    let mut ts: Option<i64> = None;
    let mut hex_sig: Option<&str> = None;
    for part in sig.split(',') {
        let (k, v) = part
            .split_once('=')
            .ok_or(WebhookVerifyError::MalformedSignature)?;
        match k.trim() {
            "t" => ts = v.trim().parse().ok(),
            "v1" => hex_sig = Some(v.trim()),
            _ => {} // ignore unknown tags for forward compat
        }
    }
    let ts = ts.ok_or(WebhookVerifyError::MalformedSignature)?;
    let hex_sig = hex_sig.ok_or(WebhookVerifyError::MalformedSignature)?;

    // Replay protection
    let now = chrono::Utc::now().timestamp();
    let tolerance = tolerance_secs.unwrap_or(DEFAULT_TOLERANCE_SECS);
    if (now - ts).abs() > tolerance {
        return Err(WebhookVerifyError::TimestampOutOfRange);
    }

    // HMAC over "<ts>.<payload>"
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| WebhookVerifyError::MalformedSignature)?;
    mac.update(ts.to_string().as_bytes());
    mac.update(b".");
    mac.update(payload);

    let expected = mac.finalize().into_bytes();
    let provided = hex::decode(hex_sig).map_err(|_| WebhookVerifyError::MalformedSignature)?;
    constant_time_eq(&expected, &provided)
        .then_some(())
        .ok_or(WebhookVerifyError::SignatureMismatch)
}

fn verify_hex(payload: &[u8], hex_sig: &str, secret: &str) -> Result<(), WebhookVerifyError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| WebhookVerifyError::MalformedSignature)?;
    mac.update(payload);
    let expected = mac.finalize().into_bytes();
    let provided = hex::decode(hex_sig).map_err(|_| WebhookVerifyError::MalformedSignature)?;
    constant_time_eq(&expected, &provided)
        .then_some(())
        .ok_or(WebhookVerifyError::SignatureMismatch)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}

/// Enqueue a webhook event into `trigger_events`.
/// The EventProcessorSubsystem will pick it up on the next pulse tick.
pub async fn enqueue_webhook_event(
    pool: &PgPool,
    trigger_id: Uuid,
    payload: serde_json::Value,
    source_ip: Option<String>,
) -> anyhow::Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO trigger_events (trigger_id, payload, source_ip) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(trigger_id)
    .bind(payload)
    .bind(source_ip)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_style_verify_roundtrip() {
        let secret = "topsecret";
        let payload = b"{\"x\":1}";
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let hex_sig = hex::encode(mac.finalize().into_bytes());
        let header = format!("sha256={hex_sig}");
        assert!(verify_signature(payload, &header, secret, None).is_ok());
    }

    #[test]
    fn tampered_payload_rejected() {
        let secret = "topsecret";
        let payload = b"{\"x\":1}";
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload);
        let hex_sig = hex::encode(mac.finalize().into_bytes());
        let header = format!("sha256={hex_sig}");
        let tampered = b"{\"x\":2}";
        assert!(matches!(
            verify_signature(tampered, &header, secret, None),
            Err(WebhookVerifyError::SignatureMismatch)
        ));
    }

    #[test]
    fn stripe_style_roundtrip() {
        let secret = "topsecret";
        let payload = b"{\"x\":1}";
        let ts = chrono::Utc::now().timestamp();
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(ts.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload);
        let hex_sig = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={ts},v1={hex_sig}");
        assert!(verify_signature(payload, &header, secret, None).is_ok());
    }

    #[test]
    fn stale_timestamp_rejected() {
        let secret = "topsecret";
        let payload = b"{}";
        let ts = chrono::Utc::now().timestamp() - 10_000; // way outside tolerance
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(ts.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload);
        let hex_sig = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={ts},v1={hex_sig}");
        assert!(matches!(
            verify_signature(payload, &header, secret, None),
            Err(WebhookVerifyError::TimestampOutOfRange)
        ));
    }
}
