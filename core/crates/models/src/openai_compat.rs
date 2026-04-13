use crate::{
    ChatRequest, ContentBlock, LlmProvider, LlmResponse, Message, MessageContent, ProviderError,
    Role, StopReason, StreamChunk, StreamResult, ToolCall, Usage,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;

const DEFAULT_MAX_TOKENS: u32 = 8192;

/// Generic adapter for any API that speaks the OpenAI chat completions format.
/// Covers OpenRouter, OpenAI, Ollama, Together, Groq, and others.
pub struct OpenAICompatProvider {
    client: Client,
    api_key: String,
    base_url: String,
    provider_name: String,
}

impl OpenAICompatProvider {
    /// OpenRouter (default).
    pub fn openrouter(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://openrouter.ai/api/v1".to_string(),
            provider_name: "openrouter".to_string(),
        }
    }

    /// OpenAI direct.
    pub fn openai(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            provider_name: "openai".to_string(),
        }
    }

    /// Local Ollama instance. No auth required.
    pub fn ollama(base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: String::new(),
            base_url: base_url.unwrap_or_else(|| "http://localhost:11434/v1".to_string()),
            provider_name: "ollama".to_string(),
        }
    }

    /// Custom OpenAI-compatible endpoint (Together, Groq, vLLM, etc.).
    pub fn custom(name: String, api_key: String, base_url: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url,
            provider_name: name,
        }
    }

    fn chat_endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    /// Build a request, attaching Bearer auth only if an API key is set.
    fn authed_request(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self
            .client
            .post(url)
            .header("content-type", "application/json");
        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }
        req
    }

    fn build_request_body(
        &self,
        request: &ChatRequest<'_>,
    ) -> Result<OpenAIRequest, ProviderError> {
        let messages: Vec<OpenAIMessage> = request
            .messages
            .iter()
            .map(|m| to_openai_message(m))
            .collect();

        let tools = request.tools.map(|ts| {
            ts.iter()
                .map(|t| OpenAITool {
                    r#type: "function".into(),
                    function: OpenAIFunction {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.input_schema.clone(),
                    },
                })
                .collect()
        });

        Ok(OpenAIRequest {
            model: request.model.to_string(),
            messages,
            tools,
            temperature: request.temperature,
            max_tokens: Some(request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS)),
            stream: request.stream,
            stream_options: if request.stream {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
        })
    }
}

fn to_openai_message(msg: &Message) -> OpenAIMessage {
    let role = match msg.role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    };

    match &msg.content {
        MessageContent::Text(s) => OpenAIMessage {
            role: role.to_string(),
            content: Some(s.clone()),
            tool_calls: None,
            tool_call_id: None,
        },
        MessageContent::Blocks(blocks) => {
            // Tool result → send as tool role with tool_call_id
            if let Some(ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            }) = blocks.first()
            {
                return OpenAIMessage {
                    role: "tool".to_string(),
                    content: Some(content.clone()),
                    tool_calls: None,
                    tool_call_id: Some(tool_use_id.clone()),
                };
            }

            // Assistant message possibly containing tool calls
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();

            for block in blocks {
                match block {
                    ContentBlock::Text { text } => text_parts.push(text.clone()),
                    ContentBlock::ToolUse { id, name, input } => {
                        tool_calls.push(OpenAIToolCall {
                            id: id.clone(),
                            r#type: "function".into(),
                            function: OpenAIFunctionCall {
                                name: name.clone(),
                                arguments: serde_json::to_string(&input).unwrap_or_default(),
                            },
                        });
                    }
                    _ => {}
                }
            }

            OpenAIMessage {
                role: role.to_string(),
                content: if text_parts.is_empty() {
                    None
                } else {
                    Some(text_parts.join(""))
                },
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                tool_call_id: None,
            }
        }
    }
}

fn parse_stop_reason(reason: Option<&str>) -> StopReason {
    match reason {
        Some("stop") => StopReason::EndTurn,
        Some("tool_calls") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        Some(other) => StopReason::Unknown(other.to_string()),
        None => StopReason::EndTurn,
    }
}

fn parse_response(resp: OpenAIResponse) -> LlmResponse {
    let choice = resp.choices.into_iter().next().unwrap_or_default();

    let tool_calls = choice
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .map(|tc| ToolCall {
            id: tc.id,
            name: tc.function.name,
            input: serde_json::from_str(&tc.function.arguments).unwrap_or_default(),
        })
        .collect();

    LlmResponse {
        content: choice.message.content.unwrap_or_default(),
        tool_calls,
        model: resp.model,
        usage: Usage {
            input_tokens: resp.usage.map(|u| u.prompt_tokens).unwrap_or(0),
            output_tokens: resp.usage.map(|u| u.completion_tokens).unwrap_or(0),
        },
        stop_reason: parse_stop_reason(choice.finish_reason.as_deref()),
    }
}

// ---------------------------------------------------------------------------
// Handle HTTP error responses
// ---------------------------------------------------------------------------

fn check_rate_limit(resp: &reqwest::Response) -> Option<ProviderError> {
    if resp.status().as_u16() == 429 {
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(|s| s * 1000);
        Some(ProviderError::RateLimited {
            retry_after_ms: retry_after,
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// LlmProvider implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl LlmProvider for OpenAICompatProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    async fn chat(&self, request: &ChatRequest<'_>) -> Result<LlmResponse, ProviderError> {
        let mut body = self.build_request_body(request)?;
        body.stream = false;
        body.stream_options = None;

        debug!(
            model = request.model,
            provider = self.provider_name,
            "OpenAI-compatible non-streaming request"
        );

        let resp = self
            .authed_request(&self.chat_endpoint())
            .json(&body)
            .send()
            .await?;

        if let Some(err) = check_rate_limit(&resp) {
            return Err(err);
        }
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message });
        }

        let api_resp: OpenAIResponse = resp
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

        debug!(
            model = request.model,
            provider = self.provider_name,
            "OpenAI-compatible streaming request"
        );

        let http_request = self.authed_request(&self.chat_endpoint()).json(&body);

        let mut es = EventSource::new(http_request)
            .map_err(|e| ProviderError::Other(format!("failed to open SSE stream: {e}")))?;

        let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        tokio::spawn(async move {
            while let Some(event_result) = es.next().await {
                match event_result {
                    Ok(Event::Open) => {}
                    Ok(Event::Message(msg)) => {
                        if msg.data == "[DONE]" {
                            es.close();
                            return;
                        }

                        let parsed: serde_json::Value = match serde_json::from_str(&msg.data) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        if let Some(chunk) = parse_openai_stream_chunk(&parsed) {
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
        match self.provider_name.as_str() {
            "openai" => {
                let m = model.strip_prefix("openai/").unwrap_or(model);
                m.starts_with("gpt-") || m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4")
            }
            "ollama" => {
                // Ollama serves whatever models are pulled locally.
                // Accept anything without a provider prefix, or with "ollama/".
                !model.contains('/') || model.starts_with("ollama/")
            }
            // openrouter, custom — accept everything
            _ => true,
        }
    }

    fn cost_per_token(&self, model: &str) -> (f64, f64) {
        if self.provider_name == "ollama" {
            return (0.0, 0.0); // local models are free
        }

        match model {
            m if m.contains("claude-opus") => (15.0 / 1e6, 75.0 / 1e6),
            m if m.contains("claude-sonnet") => (3.0 / 1e6, 15.0 / 1e6),
            m if m.contains("claude-haiku") => (0.25 / 1e6, 1.25 / 1e6),
            m if m.contains("gpt-4o") => (2.5 / 1e6, 10.0 / 1e6),
            m if m.contains("gpt-4-turbo") => (10.0 / 1e6, 30.0 / 1e6),
            m if m.contains("gpt-4.1") => (2.0 / 1e6, 8.0 / 1e6),
            _ => (0.0, 0.0),
        }
    }
}

// ---------------------------------------------------------------------------
// SSE stream chunk parsing
// ---------------------------------------------------------------------------

fn parse_openai_stream_chunk(parsed: &serde_json::Value) -> Option<StreamChunk> {
    // Check for usage in the final chunk
    if let Some(usage) = parsed.get("usage") {
        if usage.is_object() {
            let input = usage
                .get("prompt_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let output = usage
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            if input > 0 || output > 0 {
                let stop = parsed
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("finish_reason"))
                    .and_then(|v| v.as_str());
                return Some(StreamChunk::Done {
                    usage: Usage {
                        input_tokens: input,
                        output_tokens: output,
                    },
                    stop_reason: parse_stop_reason(stop),
                });
            }
        }
    }

    let choice = parsed.get("choices")?.get(0)?;
    let delta = choice.get("delta")?;

    // finish_reason without content
    if let Some(reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
        if (reason == "stop" || reason == "tool_calls" || reason == "length")
            && delta.get("content").is_none()
            && delta.get("tool_calls").is_none()
        {
            return Some(StreamChunk::Done {
                usage: Usage::default(),
                stop_reason: parse_stop_reason(Some(reason)),
            });
        }
    }

    // Text delta
    if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            return Some(StreamChunk::Text(text.to_string()));
        }
    }

    // Tool call deltas
    if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
        if let Some(tc) = tool_calls.first() {
            let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let func = tc.get("function")?;

            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                if !name.is_empty() {
                    return Some(StreamChunk::ToolCallStart {
                        id: id.to_string(),
                        name: name.to_string(),
                    });
                }
            }

            if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                if !args.is_empty() {
                    return Some(StreamChunk::ToolCallDelta {
                        id: id.to_string(),
                        json_delta: args.to_string(),
                    });
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// OpenAI-compatible wire types (private)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct OpenAIToolCall {
    id: String,
    r#type: String,
    function: OpenAIFunctionCall,
}

#[derive(Serialize, Deserialize, Clone)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct OpenAITool {
    r#type: String,
    function: OpenAIFunction,
}

#[derive(Serialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize, Default)]
struct OpenAIChoice {
    message: OpenAIChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct OpenAIChoiceMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Deserialize, Clone, Copy)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}
