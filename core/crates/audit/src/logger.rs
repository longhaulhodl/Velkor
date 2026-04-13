use crate::{AuditEntry, AuditRecord};
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use tracing::{debug, error};
use uuid::Uuid;

/// Append-only audit logger backed by the partitioned `audit_log` table.
///
/// Per PRD Section 3.3, no UPDATE or DELETE operations are performed on
/// `audit_log` — retention is handled by dropping partitions.
#[derive(Clone)]
pub struct AuditLogger {
    pool: PgPool,
}

impl AuditLogger {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert an audit entry. Returns the generated ID + timestamp.
    pub async fn log(&self, entry: &AuditEntry) -> Result<Uuid, AuditError> {
        let event_type = entry.event_type.as_str();

        // cost_usd is stored as NUMERIC(10,6) — bind as String to avoid
        // floating-point precision issues with sqlx's Decimal handling.
        let cost_str = entry.cost_usd.map(|c| format!("{c:.6}"));

        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO audit_log
                (event_type, user_id, agent_id, conversation_id, details,
                 model_used, tokens_input, tokens_output, cost_usd,
                 ip_address, request_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                    $9::NUMERIC, $10::INET, $11)
            RETURNING id
            "#,
        )
        .bind(event_type)
        .bind(entry.user_id)
        .bind(&entry.agent_id)
        .bind(entry.conversation_id)
        .bind(&entry.details)
        .bind(&entry.model_used)
        .bind(entry.tokens_input)
        .bind(entry.tokens_output)
        .bind(&cost_str)
        .bind(&entry.ip_address)
        .bind(entry.request_id)
        .fetch_one(&self.pool)
        .await?;

        debug!(%id, event_type, "Audit event logged");
        Ok(id)
    }

    /// Fire-and-forget: spawns a background task to log the entry.
    /// Audit failures are logged as errors but never propagate to the caller.
    pub fn log_async(&self, entry: AuditEntry) {
        let this = self.clone();
        tokio::spawn(async move {
            if let Err(e) = this.log(&entry).await {
                error!(
                    error = %e,
                    event_type = %entry.event_type,
                    "Failed to write audit log entry"
                );
            }
        });
    }

    /// Query audit logs with optional filters. Results ordered by timestamp DESC.
    pub async fn search(
        &self,
        filter: &AuditFilter,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<AuditRecord>, AuditError> {
        let limit_i64 = limit as i64;
        let offset_i64 = offset as i64;
        let event_type_str = filter.event_type.as_deref();

        let rows = sqlx::query_as::<_, AuditRow>(
            r#"
            SELECT id, timestamp, event_type, user_id, agent_id,
                   conversation_id, details, model_used,
                   tokens_input, tokens_output,
                   cost_usd::TEXT AS cost_usd_text,
                   host(ip_address) AS ip_address_text,
                   request_id
            FROM audit_log
            WHERE ($1::UUID IS NULL OR user_id = $1)
              AND ($2::TEXT IS NULL OR event_type = $2)
              AND ($3::UUID IS NULL OR conversation_id = $3)
              AND ($4::UUID IS NULL OR request_id = $4)
              AND ($5::TIMESTAMPTZ IS NULL OR timestamp >= $5)
              AND ($6::TIMESTAMPTZ IS NULL OR timestamp <= $6)
            ORDER BY timestamp DESC
            LIMIT $7 OFFSET $8
            "#,
        )
        .bind(filter.user_id)
        .bind(event_type_str)
        .bind(filter.conversation_id)
        .bind(filter.request_id)
        .bind(filter.from)
        .bind(filter.to)
        .bind(limit_i64)
        .bind(offset_i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_record()).collect())
    }

    /// Count audit entries matching a filter (for pagination).
    pub async fn count(&self, filter: &AuditFilter) -> Result<u64, AuditError> {
        let event_type_str = filter.event_type.as_deref();

        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM audit_log
            WHERE ($1::UUID IS NULL OR user_id = $1)
              AND ($2::TEXT IS NULL OR event_type = $2)
              AND ($3::UUID IS NULL OR conversation_id = $3)
              AND ($4::UUID IS NULL OR request_id = $4)
              AND ($5::TIMESTAMPTZ IS NULL OR timestamp >= $5)
              AND ($6::TIMESTAMPTZ IS NULL OR timestamp <= $6)
            "#,
        )
        .bind(filter.user_id)
        .bind(event_type_str)
        .bind(filter.conversation_id)
        .bind(filter.request_id)
        .bind(filter.from)
        .bind(filter.to)
        .fetch_one(&self.pool)
        .await?;

        Ok(count as u64)
    }

    /// Health check.
    pub async fn health(&self) -> Result<bool, AuditError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Filter for querying audit logs
// ---------------------------------------------------------------------------

/// Optional filters for searching audit logs.
#[derive(Debug, Default, Clone)]
pub struct AuditFilter {
    pub user_id: Option<Uuid>,
    pub event_type: Option<String>,
    pub conversation_id: Option<Uuid>,
    pub request_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// Internal row type for sqlx mapping
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct AuditRow {
    id: Uuid,
    timestamp: DateTime<Utc>,
    event_type: String,
    user_id: Option<Uuid>,
    agent_id: Option<String>,
    conversation_id: Option<Uuid>,
    details: JsonValue,
    model_used: Option<String>,
    tokens_input: Option<i32>,
    tokens_output: Option<i32>,
    cost_usd_text: Option<String>,
    ip_address_text: Option<String>,
    request_id: Option<Uuid>,
}

impl AuditRow {
    fn into_record(self) -> AuditRecord {
        AuditRecord {
            id: self.id,
            timestamp: self.timestamp,
            event_type: self.event_type,
            user_id: self.user_id,
            agent_id: self.agent_id,
            conversation_id: self.conversation_id,
            details: self.details,
            model_used: self.model_used,
            tokens_input: self.tokens_input,
            tokens_output: self.tokens_output,
            cost_usd: self.cost_usd_text.and_then(|s| s.parse::<f64>().ok()),
            ip_address: self.ip_address_text,
            request_id: self.request_id,
        }
    }
}
