use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tracing::debug;
use velkor_memory::service::{MemoryService, StoreResult};
use velkor_memory::{MemoryCategory, MemoryScope};

// ---------------------------------------------------------------------------
// memory_store
// ---------------------------------------------------------------------------

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
        "Store a durable fact for recall in future conversations. Before storing, distill \
         the information into a concise, standalone statement — never store raw conversation \
         text or quotes. Assign an importance score (1-10) reflecting long-term value.\n\n\
         STORE these:\n\
         - User identity: name, role, team, company, timezone\n\
         - Preferences: communication style, tools, frameworks, languages\n\
         - Technical environment: stack, deployment setup, infrastructure\n\
         - Project facts: names, goals, key decisions, architecture choices\n\
         - Procedures: workflows, deployment steps, how-to knowledge\n\
         - Relationships: who works with whom, team structure\n\n\
         DO NOT STORE:\n\
         - Ephemeral queries (weather, sports scores, news)\n\
         - Debugging output, error messages, stack traces\n\
         - Greetings, small talk, pleasantries\n\
         - Raw conversation snippets or direct quotes\n\
         - Information only relevant to the current conversation\n\
         - Anything the user says is temporary\n\n\
         The system will automatically deduplicate — if a near-identical memory exists, \
         it will be updated rather than creating a duplicate."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "A concise, standalone factual statement. Not a quote or conversation snippet."
                },
                "category": {
                    "type": "string",
                    "enum": ["fact", "preference", "project", "procedure", "relationship"],
                    "description": "Category of the memory. Required — if the information doesn't fit a category, it probably shouldn't be stored."
                },
                "importance": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 10,
                    "description": "How important is this for future conversations? 1-2: trivial, 3-4: minor, 5-6: moderate, 7-8: important, 9-10: critical identity/preference fact. Memories below 3 are rejected."
                },
                "scope": {
                    "type": "string",
                    "enum": ["personal", "shared"],
                    "description": "Scope: personal (user-specific) or shared (all users). Default: personal"
                }
            },
            "required": ["content", "category", "importance"]
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'content' field".into()))?;

        let category = input
            .get("category")
            .and_then(|v| v.as_str())
            .and_then(parse_category);

        let importance = input
            .get("importance")
            .and_then(|v| v.as_i64())
            .unwrap_or(5) as i16;

        let scope = input
            .get("scope")
            .and_then(|v| v.as_str())
            .map(parse_scope)
            .unwrap_or(MemoryScope::Personal);

        debug!(content_len = content.len(), ?scope, ?category, importance, "Storing memory via tool");

        let result = self
            .memory
            .store(
                ctx.user_id,
                content,
                scope,
                category,
                Some(ctx.conversation_id),
                importance,
            )
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("memory store failed: {e}")))?;

        match result {
            StoreResult::Created(id) => Ok(ToolResult::success(format!(
                "Memory stored (id: {id}, importance: {importance})"
            ))),
            StoreResult::Updated(id) => Ok(ToolResult::success(format!(
                "Similar memory already existed — updated existing memory (id: {id}) instead of creating duplicate"
            ))),
            StoreResult::Rejected { reason } => Ok(ToolResult::success(format!(
                "Memory rejected: {reason}"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// memory_search
// ---------------------------------------------------------------------------

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
        "Search stored memories using natural language. Returns relevant memories ranked \
         by hybrid FTS + semantic similarity. Use this at the start of conversations to \
         recall context about the user, their projects, and preferences. Also use when \
         the user references something from a previous conversation."
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
                    "{}. [{}] (importance: {}, score: {:.3}) {}",
                    i + 1,
                    r.category
                        .map(|c| c.as_str().to_string())
                        .unwrap_or_else(|| "uncategorized".to_string()),
                    r.importance,
                    r.score,
                    r.content
                )
            })
            .collect();

        Ok(ToolResult::success(formatted.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// memory_update
// ---------------------------------------------------------------------------

pub struct MemoryUpdateTool {
    memory: Arc<MemoryService>,
}

impl MemoryUpdateTool {
    pub fn new(memory: Arc<MemoryService>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryUpdateTool {
    fn name(&self) -> &str {
        "memory_update"
    }

    fn description(&self) -> &str {
        "Update an existing memory with new content. Use this when a fact has changed \
         (e.g., user changed roles, project goals shifted) rather than creating a new \
         memory. The embedding is automatically regenerated. Provide the memory ID from \
         a previous memory_search result."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "UUID of the memory to update (from memory_search results)"
                },
                "content": {
                    "type": "string",
                    "description": "The updated content — a concise, standalone factual statement"
                }
            },
            "required": ["id", "content"]
        })
    }

    async fn execute(&self, input: JsonValue, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let id_str = input
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'id' field".into()))?;

        let id: uuid::Uuid = id_str
            .parse()
            .map_err(|_| ToolError::InvalidInput(format!("invalid UUID: {id_str}")))?;

        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'content' field".into()))?;

        debug!(%id, content_len = content.len(), "Updating memory via tool");

        self.memory
            .update(id, content)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("memory update failed: {e}")))?;

        Ok(ToolResult::success(format!(
            "Memory updated successfully (id: {id})"
        )))
    }
}

// ---------------------------------------------------------------------------
// memory_forget
// ---------------------------------------------------------------------------

pub struct MemoryForgetTool {
    memory: Arc<MemoryService>,
}

impl MemoryForgetTool {
    pub fn new(memory: Arc<MemoryService>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryForgetTool {
    fn name(&self) -> &str {
        "memory_forget"
    }

    fn description(&self) -> &str {
        "Delete a stored memory. Use when the user explicitly asks you to forget something, \
         or when you discover a memory is outdated, incorrect, or no longer relevant. \
         Provide the memory ID from a memory_search result. This is a soft-delete — \
         the memory is marked as deleted but retained for compliance/audit purposes."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "UUID of the memory to delete (from memory_search results)"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: JsonValue, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let id_str = input
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'id' field".into()))?;

        let id: uuid::Uuid = id_str
            .parse()
            .map_err(|_| ToolError::InvalidInput(format!("invalid UUID: {id_str}")))?;

        debug!(%id, "Deleting memory via tool");

        self.memory
            .delete(id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("memory forget failed: {e}")))?;

        Ok(ToolResult::success(format!(
            "Memory deleted (id: {id})"
        )))
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
