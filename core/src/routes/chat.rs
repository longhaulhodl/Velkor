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
use tracing::{debug, warn};
use uuid::Uuid;
use velkor_models::{Message, MessageContent, Role};
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

    // Ensure the conversation record exists.
    // Can't use ON CONFLICT (id) on a partitioned table, so check first.
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = $1)",
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap_or(false);

    if !exists {
        let _ = sqlx::query(
            r#"
            INSERT INTO conversations (id, user_id, agent_id, title, started_at)
            VALUES ($1, $2, $3, $4, now())
            "#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .bind(&req.agent_id)
        .bind(truncate_title(&user_message))
        .execute(&pool)
        .await
        .map_err(|e| warn!(error = %e, "Failed to create conversation record"));
    }

    // Load conversation history BEFORE persisting new message (ReAct loop adds it).
    // Fetch in reverse chronological order so we can apply a token budget,
    // then reverse to chronological for the model.
    let mut context = ConversationContext::new(
        conversation_id,
        user_id,
        &req.agent_id,
    );

    let history = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT role, content FROM messages
        WHERE conversation_id = $1
        ORDER BY created_at DESC
        LIMIT 200
        "#,
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    // Apply token budget: reserve ~20k tokens for system prompt, tools, and response.
    // Rough estimate: 1 token ≈ 4 characters.
    const MAX_HISTORY_CHARS: usize = 80_000 * 4; // ~80k tokens for history
    let mut char_budget = MAX_HISTORY_CHARS;
    let mut windowed: Vec<(String, String)> = Vec::new();

    for (role, content) in history {
        let msg_chars = content.len() + 20; // small overhead for role/framing
        if msg_chars > char_budget {
            break;
        }
        char_budget -= msg_chars;
        windowed.push((role, content));
    }
    windowed.reverse(); // back to chronological order

    let history_len = windowed.len();
    for (role, content) in windowed {
        let msg = match role.as_str() {
            "user" => Message { role: Role::User, content: MessageContent::Text(content) },
            "assistant" => Message { role: Role::Assistant, content: MessageContent::Text(content) },
            _ => continue,
        };
        context.push(msg);
    }
    debug!(conversation_id = %conversation_id, history_len, "Loaded conversation history");

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
