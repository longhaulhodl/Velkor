use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::post,
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::StreamExt;
use tracing::warn;
use uuid::Uuid;
use velkor_runtime::context::ConversationContext;
use velkor_runtime::react::RuntimeEvent;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", post(chat_stream))
}

#[derive(Deserialize)]
struct ChatRequest {
    user_id: Uuid,
    agent_id: String,
    conversation_id: Uuid,
    message: String,
}

/// Streaming chat endpoint. Returns SSE events that the TS gateway
/// forwards over the WebSocket.
async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let pool = state.pool.clone();
    let user_id = req.user_id;
    let conversation_id = req.conversation_id;
    let user_message = req.message.clone();

    // Ensure the conversation record exists (upsert — ignore conflict if already created)
    let _ = sqlx::query(
        r#"
        INSERT INTO conversations (id, user_id, agent_id, title, started_at)
        VALUES ($1, $2, $3, $4, now())
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(conversation_id)
    .bind(user_id)
    .bind(&req.agent_id)
    .bind(truncate_title(&user_message))
    .execute(&pool)
    .await
    .map_err(|e| warn!(error = %e, "Failed to create conversation record"));

    // Persist the user message
    let _ = sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES ($1, $2, 'user', $3, now())",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(&user_message)
    .execute(&pool)
    .await
    .map_err(|e| warn!(error = %e, "Failed to persist user message"));

    let context = ConversationContext::new(
        conversation_id,
        user_id,
        &req.agent_id,
    );

    // Start the streaming ReAct loop
    let runtime = Arc::clone(&state.runtime);
    let event_stream = runtime.run_stream(req.message, context);

    // Collect assistant response text so we can persist it when the stream ends
    let response_pool = pool.clone();

    // Map RuntimeEvent → SSE Event, also persist assistant response on Done
    let mut accumulated_text = String::new();
    let sse_stream = event_stream.map(move |event| {
        let sse = match &event {
            RuntimeEvent::Text(text) => {
                accumulated_text.push_str(text);
                Event::default().event("text").data(text.clone())
            }
            RuntimeEvent::ToolStatus { tool, status } => {
                let status_str = match status {
                    velkor_runtime::react::ToolStatusKind::Started => "started",
                    velkor_runtime::react::ToolStatusKind::Completed => "completed",
                    velkor_runtime::react::ToolStatusKind::Failed => "failed",
                };
                Event::default()
                    .event("tool_status")
                    .data(
                        serde_json::json!({ "tool": tool, "status": status_str }).to_string(),
                    )
            }
            RuntimeEvent::Done {
                request_id,
                iterations,
                usage,
            } => {
                // Persist assistant response in background
                let text = accumulated_text.clone();
                let pool = response_pool.clone();
                let conv_id = conversation_id;
                tokio::spawn(async move {
                    let _ = sqlx::query(
                        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES ($1, $2, 'assistant', $3, now())",
                    )
                    .bind(Uuid::new_v4())
                    .bind(conv_id)
                    .bind(&text)
                    .execute(&pool)
                    .await
                    .map_err(|e| warn!(error = %e, "Failed to persist assistant message"));

                    // Update conversation metadata
                    let _ = sqlx::query(
                        "UPDATE conversations SET message_count = (SELECT count(*) FROM messages WHERE conversation_id = $1) WHERE id = $1",
                    )
                    .bind(conv_id)
                    .execute(&pool)
                    .await;
                });

                Event::default().event("done").data(
                    serde_json::json!({
                        "request_id": request_id.to_string(),
                        "iterations": iterations,
                        "usage": {
                            "input_tokens": usage.input_tokens,
                            "output_tokens": usage.output_tokens,
                        }
                    })
                    .to_string(),
                )
            }
            RuntimeEvent::Error(msg) => Event::default().event("error").data(msg.clone()),
        };
        Ok::<_, Infallible>(sse)
    });

    Sse::new(sse_stream)
}

/// Truncate message text to use as a conversation title.
fn truncate_title(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= 80 {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..77])
    }
}
