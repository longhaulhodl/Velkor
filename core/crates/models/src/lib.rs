pub mod anthropic;
pub mod embeddings;
pub mod openai_compat;
pub mod router;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio_stream::Stream;

// ---------------------------------------------------------------------------
// Core message types — provider-agnostic
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl MessageContent {
    pub fn as_text(&self) -> &str {
        match self {
            MessageContent::Text(s) => s,
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .find_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .unwrap_or(""),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

// ---------------------------------------------------------------------------
// Tool schema — passed to the model so it knows what tools are available
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Tool calls parsed from model responses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

// ---------------------------------------------------------------------------
// LLM response — unified across all providers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub model: String,
    pub usage: Usage,
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl Usage {
    pub fn cost(&self, input_price: f64, output_price: f64) -> f64 {
        (self.input_tokens as f64 * input_price) + (self.output_tokens as f64 * output_price)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    Unknown(String),
}

// ---------------------------------------------------------------------------
// Streaming chunk — yielded during streamed responses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// A piece of text content
    Text(String),
    /// A complete tool call (accumulated from deltas)
    ToolCallStart {
        id: String,
        name: String,
    },
    /// Incremental JSON input for an in-progress tool call
    ToolCallDelta {
        id: String,
        json_delta: String,
    },
    /// Signals the response is complete; carries final usage stats
    Done {
        usage: Usage,
        stop_reason: StopReason,
    },
    /// Provider-level error mid-stream
    Error(String),
}

pub type StreamResult = Pin<Box<dyn Stream<Item = StreamChunk> + Send>>;

// ---------------------------------------------------------------------------
// LlmProvider trait — the abstraction every provider implements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [Message],
    pub tools: Option<&'a [ToolSchema]>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("deserialization error: {0}")]
    Deserialize(String),
    #[error("model not supported: {0}")]
    UnsupportedModel(String),
    #[error("rate limited, retry after {retry_after_ms:?}ms")]
    RateLimited { retry_after_ms: Option<u64> },
    #[error("{0}")]
    Other(String),
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Human-readable provider name (e.g. "anthropic", "openrouter")
    fn name(&self) -> &str;

    /// Non-streaming chat completion. Returns a fully-assembled response.
    async fn chat(&self, request: &ChatRequest<'_>) -> Result<LlmResponse, ProviderError>;

    /// Streaming chat completion. Returns a stream of chunks.
    async fn chat_stream(&self, request: &ChatRequest<'_>) -> Result<StreamResult, ProviderError>;

    /// Whether this provider handles the given model identifier.
    fn supports_model(&self, model: &str) -> bool;

    /// Per-token costs: (input_cost_per_token, output_cost_per_token).
    /// Returns (0, 0) if pricing is unknown.
    fn cost_per_token(&self, model: &str) -> (f64, f64);
}
