use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;
use velkor_documents::store::DocumentStore;

// ---------------------------------------------------------------------------
// document_read
// ---------------------------------------------------------------------------

/// Read a document's extracted text content by ID.
pub struct DocumentReadTool {
    store: Arc<DocumentStore>,
}

impl DocumentReadTool {
    pub fn new(store: Arc<DocumentStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for DocumentReadTool {
    fn name(&self) -> &str {
        "document_read"
    }

    fn description(&self) -> &str {
        "Read the text content of an uploaded document by its ID. Returns the extracted text (not the raw file). Use document_search first to find relevant documents."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "document_id": {
                    "type": "string",
                    "description": "The UUID of the document to read"
                }
            },
            "required": ["document_id"]
        })
    }

    async fn execute(&self, input: JsonValue, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let id_str = input
            .get("document_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'document_id' field".into()))?;

        let id = Uuid::parse_str(id_str)
            .map_err(|_| ToolError::InvalidInput(format!("invalid UUID: {id_str}")))?;

        debug!(%id, "Reading document via tool");

        let doc = self
            .store
            .get(id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("document read failed: {e}")))?
            .ok_or_else(|| ToolError::ExecutionFailed(format!("document not found: {id}")))?;

        let content = doc.content_text.unwrap_or_else(|| {
            format!(
                "[No text content extracted for '{}'. The file format may not be supported for text extraction yet.]",
                doc.filename
            )
        });

        let header = format!(
            "Document: {}\nType: {}\nUploaded: {}\n---\n",
            doc.filename,
            doc.mime_type.as_deref().unwrap_or("unknown"),
            doc.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
        );

        Ok(ToolResult::success(format!("{header}{content}")))
    }
}

// ---------------------------------------------------------------------------
// document_search
// ---------------------------------------------------------------------------

/// Search documents by content using hybrid FTS + vector search.
pub struct DocumentSearchTool {
    store: Arc<DocumentStore>,
}

impl DocumentSearchTool {
    pub fn new(store: Arc<DocumentStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for DocumentSearchTool {
    fn name(&self) -> &str {
        "document_search"
    }

    fn description(&self) -> &str {
        "Search uploaded documents by content. Returns matching documents ranked by relevance. Use this to find information across all uploaded files."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language search query"
                },
                "workspace_id": {
                    "type": "string",
                    "description": "Workspace UUID to search within"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 5)",
                    "minimum": 1,
                    "maximum": 20
                }
            },
            "required": ["query", "workspace_id"]
        })
    }

    async fn execute(&self, input: JsonValue, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'query' field".into()))?;

        let workspace_str = input
            .get("workspace_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'workspace_id' field".into()))?;

        let workspace_id = Uuid::parse_str(workspace_str)
            .map_err(|_| ToolError::InvalidInput(format!("invalid UUID: {workspace_str}")))?;

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        debug!(query, %workspace_id, limit, "Searching documents via tool");

        let results = self
            .store
            .search(query, workspace_id, limit)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("document search failed: {e}")))?;

        if results.is_empty() {
            return Ok(ToolResult::success("No matching documents found."));
        }

        let formatted: Vec<String> = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "{}. **{}** (id: {}, score: {:.3})\n   {}\n   Type: {} | Uploaded: {}",
                    i + 1,
                    r.filename,
                    r.id,
                    r.score,
                    if r.snippet.is_empty() {
                        "[no preview]".to_string()
                    } else {
                        r.snippet.clone()
                    },
                    r.mime_type.as_deref().unwrap_or("unknown"),
                    r.created_at.format("%Y-%m-%d"),
                )
            })
            .collect();

        Ok(ToolResult::success(formatted.join("\n\n")))
    }
}
