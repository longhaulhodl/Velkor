use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tracing::debug;
use velkor_memory::service::MemoryService;
use velkor_memory::{MemoryCategory, MemoryScope};

// ---------------------------------------------------------------------------
// memory_store
// ---------------------------------------------------------------------------

/// Stores a memory for later recall. The agent uses this to save
/// facts, preferences, or context it wants to remember.
pub struct MemoryStoreTool {
    memory: Arc<MemoryService>,
}

impl MemoryStoreTool {
    pub fn new(memory: Arc<MemoryService>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryStoreTool {
    fn name(&self) -> &str {
        "memory_store"
    }

    fn description(&self) -> &str {
        "Store a memory for later recall. Use this to save important facts, user preferences, project details, or anything the user might want you to remember across conversations."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The information to remember"
                },
                "category": {
                    "type": "string",
                    "enum": ["fact", "preference", "project", "procedure", "relationship"],
                    "description": "Category of the memory (optional)"
                },
                "scope": {
                    "type": "string",
                    "enum": ["personal", "shared"],
                    "description": "Scope: personal (user-specific) or shared (all users). Default: personal"
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'content' field".into()))?;

        let scope = input
            .get("scope")
            .and_then(|v| v.as_str())
            .map(parse_scope)
            .unwrap_or(MemoryScope::Personal);

        let category = input
            .get("category")
            .and_then(|v| v.as_str())
            .and_then(parse_category);

        debug!(content_len = content.len(), ?scope, ?category, "Storing memory via tool");

        let id = self
            .memory
            .store(
                ctx.user_id,
                content,
                scope,
                category,
                Some(ctx.conversation_id),
            )
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("memory store failed: {e}")))?;

        Ok(ToolResult::success(format!(
            "Memory stored successfully (id: {id})"
        )))
    }
}

// ---------------------------------------------------------------------------
// memory_search
// ---------------------------------------------------------------------------

/// Searches stored memories by semantic similarity.
pub struct MemorySearchTool {
    memory: Arc<MemoryService>,
}

impl MemorySearchTool {
    pub fn new(memory: Arc<MemoryService>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search stored memories using natural language. Returns relevant memories ranked by relevance. Use this to recall facts, preferences, or context from previous conversations."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language search query"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 5)",
                    "minimum": 1,
                    "maximum": 20
                },
                "scope": {
                    "type": "string",
                    "enum": ["personal", "shared"],
                    "description": "Search scope. Default: personal"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'query' field".into()))?;

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let scope = input
            .get("scope")
            .and_then(|v| v.as_str())
            .map(parse_scope)
            .unwrap_or(MemoryScope::Personal);

        debug!(query, limit, ?scope, "Searching memories via tool");

        let results = self
            .memory
            .search(query, scope, ctx.user_id, limit)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("memory search failed: {e}")))?;

        if results.is_empty() {
            return Ok(ToolResult::success("No matching memories found."));
        }

        let formatted: Vec<String> = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "{}. [{}] (score: {:.3}) {}",
                    i + 1,
                    r.category
                        .map(|c| c.as_str().to_string())
                        .unwrap_or_else(|| "uncategorized".to_string()),
                    r.score,
                    r.content
                )
            })
            .collect();

        Ok(ToolResult::success(formatted.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_scope(s: &str) -> MemoryScope {
    match s {
        "shared" => MemoryScope::Shared,
        "org" => MemoryScope::Org,
        _ => MemoryScope::Personal,
    }
}

fn parse_category(s: &str) -> Option<MemoryCategory> {
    match s {
        "fact" => Some(MemoryCategory::Fact),
        "preference" => Some(MemoryCategory::Preference),
        "project" => Some(MemoryCategory::Project),
        "procedure" => Some(MemoryCategory::Procedure),
        "relationship" => Some(MemoryCategory::Relationship),
        _ => None,
    }
}
