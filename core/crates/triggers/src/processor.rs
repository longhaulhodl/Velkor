//! Event-processor subsystem.
//!
//! Dequeues pending rows from `trigger_events`, resolves the owning trigger,
//! renders the prompt template against the event payload, and runs the
//! configured agent. Mirrors scheduler's `execute_schedule` pattern.

use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;
use velkor_pulse::{PulseSubsystem, SubsystemTickResult};
use velkor_runtime::context::ConversationContext;
use velkor_runtime::react::AgentRuntime;

use crate::template;

/// Max events drained per tick — bounds per-tick wall time.
const BATCH_SIZE: i64 = 10;
/// Max retries before a failed event is permanently marked 'failed'.
const MAX_ATTEMPTS: i32 = 3;

pub struct EventProcessorSubsystem {
    pool: PgPool,
    runtime: Arc<AgentRuntime>,
}

impl EventProcessorSubsystem {
    pub fn new(pool: PgPool, runtime: Arc<AgentRuntime>) -> Self {
        Self { pool, runtime }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PendingEvent {
    event_id: Uuid,
    trigger_id: Uuid,
    payload: serde_json::Value,
    attempts: i32,
    user_id: Uuid,
    name: String,
    agent_id: String,
    prompt_template: String,
}

#[async_trait]
impl PulseSubsystem for EventProcessorSubsystem {
    fn name(&self) -> &str {
        "trigger_processor"
    }

    async fn tick(&self) -> anyhow::Result<SubsystemTickResult> {
        // Atomically claim up to BATCH_SIZE pending events and join trigger metadata.
        // SKIP LOCKED makes this safe if multiple processors run in parallel (future-proofing).
        let events = sqlx::query_as::<_, PendingEvent>(
            "WITH claimed AS ( \
                UPDATE trigger_events SET \
                    status = 'processing', \
                    started_at = now(), \
                    attempts = attempts + 1 \
                WHERE id IN ( \
                    SELECT te.id FROM trigger_events te \
                    JOIN triggers t ON t.id = te.trigger_id \
                    WHERE te.status = 'pending' AND t.is_active = TRUE \
                    ORDER BY te.created_at ASC \
                    LIMIT $1 \
                    FOR UPDATE OF te SKIP LOCKED \
                ) \
                RETURNING id, trigger_id, payload, attempts \
             ) \
             SELECT c.id AS event_id, c.trigger_id, c.payload, c.attempts, \
                    t.user_id, t.name, t.agent_id, t.prompt_template \
             FROM claimed c JOIN triggers t ON t.id = c.trigger_id",
        )
        .bind(BATCH_SIZE)
        .fetch_all(&self.pool)
        .await?;

        let checked = events.len() as u32;
        if checked == 0 {
            return Ok(SubsystemTickResult {
                name: self.name().to_string(),
                checked: 0,
                processed: 0,
                failed: 0,
                duration_ms: 0,
                details: None,
            });
        }

        let mut processed = 0u32;
        let mut failed = 0u32;

        for ev in &events {
            match execute_event(&self.pool, &self.runtime, ev).await {
                Ok(()) => processed += 1,
                Err(e) => {
                    failed += 1;
                    warn!(event_id = %ev.event_id, trigger = %ev.name, error = %e, "Trigger event execution failed");
                }
            }
        }

        Ok(SubsystemTickResult {
            name: self.name().to_string(),
            checked,
            processed,
            failed,
            duration_ms: 0,
            details: None,
        })
    }
}

async fn execute_event(
    pool: &PgPool,
    runtime: &AgentRuntime,
    ev: &PendingEvent,
) -> anyhow::Result<()> {
    let prompt = template::render(&ev.prompt_template, &ev.payload);
    let conversation_id = Uuid::new_v4();

    // Seed conversation + user message rows (parallel to scheduler)
    let _ = sqlx::query(
        "INSERT INTO conversations (id, user_id, agent_id, title, started_at) \
         VALUES ($1, $2, $3, $4, now())",
    )
    .bind(conversation_id)
    .bind(ev.user_id)
    .bind(&ev.agent_id)
    .bind(format!("[Trigger] {}", ev.name))
    .execute(pool)
    .await;

    let _ = sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) \
         VALUES ($1, $2, 'user', $3, now())",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(&prompt)
    .execute(pool)
    .await;

    // Link the event to the conversation before running
    sqlx::query("UPDATE trigger_events SET conversation_id = $1 WHERE id = $2")
        .bind(conversation_id)
        .bind(ev.event_id)
        .execute(pool)
        .await?;

    let mut context = ConversationContext::new(conversation_id, ev.user_id, &ev.agent_id);

    let result = runtime.run(&prompt, &mut context).await;

    match result {
        Ok(response) => {
            let _ = sqlx::query(
                "INSERT INTO messages (id, conversation_id, role, content, created_at) \
                 VALUES ($1, $2, 'assistant', $3, now())",
            )
            .bind(Uuid::new_v4())
            .bind(conversation_id)
            .bind(&response.content)
            .execute(pool)
            .await;

            sqlx::query(
                "UPDATE trigger_events SET status = 'done', completed_at = now(), error = NULL \
                 WHERE id = $1",
            )
            .bind(ev.event_id)
            .execute(pool)
            .await?;

            sqlx::query(
                "UPDATE triggers SET \
                    last_fired_at = now(), \
                    fire_count = fire_count + 1, \
                    last_error = NULL \
                 WHERE id = $1",
            )
            .bind(ev.trigger_id)
            .execute(pool)
            .await?;

            info!(
                trigger = %ev.name,
                trigger_id = %ev.trigger_id,
                event_id = %ev.event_id,
                conversation_id = %conversation_id,
                "Trigger event processed"
            );
            Ok(())
        }
        Err(e) => {
            let err_msg = format!("{e}");
            // If we've exceeded retries, terminally fail; otherwise reset to pending for a retry.
            let terminal = ev.attempts >= MAX_ATTEMPTS;
            let new_status = if terminal { "failed" } else { "pending" };

            sqlx::query(
                "UPDATE trigger_events SET status = $1, error = $2, \
                 completed_at = CASE WHEN $1 = 'failed' THEN now() ELSE NULL END \
                 WHERE id = $3",
            )
            .bind(new_status)
            .bind(&err_msg)
            .bind(ev.event_id)
            .execute(pool)
            .await?;

            sqlx::query(
                "UPDATE triggers SET error_count = error_count + 1, last_error = $1 WHERE id = $2",
            )
            .bind(&err_msg)
            .bind(ev.trigger_id)
            .execute(pool)
            .await?;

            if terminal {
                error!(
                    trigger = %ev.name,
                    event_id = %ev.event_id,
                    attempts = ev.attempts,
                    error = %e,
                    "Trigger event terminally failed after retries"
                );
            }

            // Swallow retryable errors so other events in the batch still run
            if terminal {
                Err(anyhow::anyhow!(err_msg))
            } else {
                Ok(())
            }
        }
    }
}
