pub mod builtin;
pub mod registry;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tool result
// ---------------------------------------------------------------------------

/// The result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// The tool output content (text, JSON, etc.).
    pub content: String,
    /// Whether the tool execution resulted in an error.
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: message.into(),
            is_error: true,
        }
    }

    /// Truncated summary for audit logging.
    pub fn summary(&self, max_len: usize) -> String {
        if self.content.len() <= max_len {
            self.content.clone()
        } else {
            format!("{}…", &self.content[..max_len])
        }
    }
}

// ---------------------------------------------------------------------------
// Tool execution context
// ---------------------------------------------------------------------------

/// Context passed to tools during execution, providing identity and
/// conversation state without coupling tools to the full runtime.
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub user_id: Uuid,
    pub conversation_id: Uuid,
    pub agent_id: String,
}

// ---------------------------------------------------------------------------
// Tool trait
// ---------------------------------------------------------------------------

/// Error type for tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("timeout")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

/// A tool that agents can invoke during the ReAct loop.
///
/// Each tool provides its JSON schema (for the model) and an async execute
/// method. Tools are registered in a [`ToolRegistry`](registry::ToolRegistry).
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name matching what the model sees (e.g. "web_search").
    fn name(&self) -> &str;

    /// Human-readable description for the model.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's input parameters.
    fn input_schema(&self) -> JsonValue;

    /// Execute the tool with the given JSON input.
    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError>;
}
