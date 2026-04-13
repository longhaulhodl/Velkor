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
    let context = ConversationContext::new(
        req.conversation_id,
        req.user_id,
        &req.agent_id,
    );

    // Start the streaming ReAct loop
    let runtime = Arc::clone(&state.runtime);
    let event_stream = runtime.run_stream(req.message, context);

    // Map RuntimeEvent → SSE Event
    let sse_stream = event_stream.map(|event| {
        let sse = match event {
            RuntimeEvent::Text(text) => Event::default().event("text").data(text),
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
            } => Event::default().event("done").data(
                serde_json::json!({
                    "request_id": request_id.to_string(),
                    "iterations": iterations,
                    "usage": {
                        "input_tokens": usage.input_tokens,
                        "output_tokens": usage.output_tokens,
                    }
                })
                .to_string(),
            ),
            RuntimeEvent::Error(msg) => Event::default().event("error").data(msg),
        };
        Ok::<_, Infallible>(sse)
    });

    Sse::new(sse_stream)
}
