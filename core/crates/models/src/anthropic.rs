use crate::{
    ChatRequest, ContentBlock, LlmProvider, LlmResponse, MessageContent, ProviderError, Role,
    StopReason, StreamChunk, StreamResult, ToolCall, Usage,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 8192;

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/messages", self.base_url)
    }

    fn build_request_body(
        &self,
        request: &ChatRequest<'_>,
    ) -> Result<AnthropicRequest, ProviderError> {
        let mut system_prompt = None;
        let mut messages = Vec::new();

        for msg in request.messages {
            match msg.role {
                Role::System => {
                    system_prompt = Some(msg.content.as_text().to_string());
                }
                Role::User => {
                    messages.push(AnthropicMessage {
                        role: "user".into(),
                        content: to_anthropic_content(&msg.content),
                    });
                }
                Role::Assistant => {
                    messages.push(AnthropicMessage {
                        role: "assistant".into(),
                        content: to_anthropic_content(&msg.content),
                    });
                }
                Role::Tool => {
                    messages.push(AnthropicMessage {
                        role: "user".into(),
                        content: to_anthropic_content(&msg.content),
                    });
                }
            }
        }

        let tools = request.tools.map(|ts| {
            ts.iter()
                .map(|t| AnthropicTool {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    input_schema: t.input_schema.clone(),
                })
                .collect()
        });

        Ok(AnthropicRequest {
            model: request.model.to_string(),
            max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            system: system_prompt,
            messages,
            tools,
            temperature: request.temperature,
            stream: request.stream,
        })
    }
}

fn to_anthropic_content(content: &MessageContent) -> AnthropicContent {
    match content {
        MessageContent::Text(s) => AnthropicContent::Text(s.clone()),
        MessageContent::Blocks(blocks) => {
            let parts: Vec<serde_json::Value> = blocks
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => serde_json::json!({
                        "type": "text",
                        "text": text,
                    }),
                    ContentBlock::ToolUse { id, name, input } => serde_json::json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input,
                    }),
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": content,
                        "is_error": is_error,
                    }),
                })
                .collect();
            AnthropicContent::Blocks(parts)
        }
    }
}

fn parse_stop_reason(reason: &str) -> StopReason {
    match reason {
        "end_turn" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        other => StopReason::Unknown(other.to_string()),
    }
}

fn parse_response(resp: AnthropicResponse) -> LlmResponse {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in &resp.content {
        match block.get("type").and_then(|v| v.as_str()) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    text_parts.push(t.to_string());
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = block
                    .get("input")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                tool_calls.push(ToolCall { id, name, input });
            }
            _ => {}
        }
    }

    LlmResponse {
        content: text_parts.join(""),
        tool_calls,
        model: resp.model,
        usage: Usage {
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
        },
        stop_reason: parse_stop_reason(&resp.stop_reason),
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn chat(&self, request: &ChatRequest<'_>) -> Result<LlmResponse, ProviderError> {
        let mut body = self.build_request_body(request)?;
        body.stream = false;

        debug!(model = request.model, "Anthropic non-streaming request");

        let resp = self
            .client
            .post(self.endpoint())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(|s| s * 1000);
            return Err(ProviderError::RateLimited {
                retry_after_ms: retry_after,
            });
        }
        if !resp.status().is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message });
        }

        let api_resp: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Deserialize(e.to_string()))?;

        Ok(parse_response(api_resp))
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest<'_>,
    ) -> Result<StreamResult, ProviderError> {
        let mut body = self.build_request_body(request)?;
        body.stream = true;

        debug!(model = request.model, "Anthropic streaming request");

        let http_request = self
            .client
            .post(self.endpoint())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body);

        let mut es = EventSource::new(http_request)
            .map_err(|e| ProviderError::Other(format!("failed to open SSE stream: {e}")))?;

        let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        tokio::spawn(async move {
            while let Some(event_result) = es.next().await {
                match event_result {
                    Ok(Event::Open) => {}
                    Ok(Event::Message(msg)) => {
                        if let Some(chunk) = parse_sse_event(&msg.event, &msg.data) {
                            if tx.send(chunk).await.is_err() {
                                es.close();
                                return;
                            }
                        }
                    }
                    Err(reqwest_eventsource::Error::StreamEnded) => return,
                    Err(e) => {
                        let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                        es.close();
                        return;
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn supports_model(&self, model: &str) -> bool {
        let m = model.strip_prefix("anthropic/").unwrap_or(model);
        m.starts_with("claude-")
    }

    fn cost_per_token(&self, model: &str) -> (f64, f64) {
        let m = model.strip_prefix("anthropic/").unwrap_or(model);
        match m {
            m if m.starts_with("claude-opus") => (15.0 / 1_000_000.0, 75.0 / 1_000_000.0),
            m if m.starts_with("claude-sonnet") => (3.0 / 1_000_000.0, 15.0 / 1_000_000.0),
            m if m.starts_with("claude-haiku") => (0.25 / 1_000_000.0, 1.25 / 1_000_000.0),
            _ => (0.0, 0.0),
        }
    }
}

/// Parse an Anthropic SSE event (already split by reqwest-eventsource) into a StreamChunk.
fn parse_sse_event(event_type: &str, data: &str) -> Option<StreamChunk> {
    match event_type {
        "content_block_start" => {
            let parsed: serde_json::Value = serde_json::from_str(data).ok()?;
            let block = parsed.get("content_block")?;
            match block.get("type")?.as_str()? {
                "tool_use" => Some(StreamChunk::ToolCallStart {
                    id: block.get("id")?.as_str()?.to_string(),
                    name: block.get("name")?.as_str()?.to_string(),
                }),
                _ => None,
            }
        }
        "content_block_delta" => {
            let parsed: serde_json::Value = serde_json::from_str(data).ok()?;
            let delta = parsed.get("delta")?;
            match delta.get("type")?.as_str()? {
                "text_delta" => {
                    let text = delta.get("text")?.as_str()?.to_string();
                    Some(StreamChunk::Text(text))
                }
                "input_json_delta" => {
                    let json_delta = delta.get("partial_json")?.as_str()?.to_string();
                    let index = parsed.get("index")?.as_u64()? as usize;
                    Some(StreamChunk::ToolCallDelta {
                        id: index.to_string(),
                        json_delta,
                    })
                }
                _ => None,
            }
        }
        "message_delta" => {
            let parsed: serde_json::Value = serde_json::from_str(data).ok()?;
            let delta = parsed.get("delta")?;
            let stop = delta
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .map(parse_stop_reason)
                .unwrap_or(StopReason::EndTurn);
            let usage_obj = parsed.get("usage");
            let output_tokens = usage_obj
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            Some(StreamChunk::Done {
                usage: Usage {
                    input_tokens: 0,
                    output_tokens,
                },
                stop_reason: stop,
            })
        }
        "error" => {
            let parsed: serde_json::Value = serde_json::from_str(data).ok()?;
            let msg = parsed
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
                .to_string();
            warn!(error = %msg, "Anthropic stream error");
            Some(StreamChunk::Error(msg))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Anthropic API wire types (private)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<serde_json::Value>),
}

#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<serde_json::Value>,
    model: String,
    stop_reason: String,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}
