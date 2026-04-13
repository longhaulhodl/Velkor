pub mod extractors;
pub mod store;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A document record as stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub filename: String,
    pub mime_type: Option<String>,
    pub file_size: Option<i64>,
    pub storage_key: String,
    pub content_text: Option<String>,
    pub metadata: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_deleted: bool,
    pub legal_hold: bool,
}

/// Input for uploading a new document.
#[derive(Debug, Clone)]
pub struct NewDocument {
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub filename: String,
    pub mime_type: Option<String>,
    pub file_size: i64,
    pub storage_key: String,
    pub content_text: Option<String>,
    pub content_embedding: Option<Vec<f32>>,
    pub metadata: JsonValue,
}

/// Search result for document queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSearchResult {
    pub id: Uuid,
    pub filename: String,
    pub mime_type: Option<String>,
    pub workspace_id: Uuid,
    /// Snippet of matching text.
    pub snippet: String,
    pub score: f64,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Supported formats
// ---------------------------------------------------------------------------

/// Document formats we can extract text from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentFormat {
    PlainText,
    Markdown,
    Pdf,
    Docx,
}

impl DocumentFormat {
    /// Detect format from MIME type or filename extension.
    pub fn detect(mime_type: Option<&str>, filename: &str) -> Option<Self> {
        // Try MIME type first
        if let Some(mime) = mime_type {
            match mime {
                "text/plain" => return Some(Self::PlainText),
                "text/markdown" => return Some(Self::Markdown),
                "application/pdf" => return Some(Self::Pdf),
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                    return Some(Self::Docx)
                }
                _ => {}
            }
        }

        // Fall back to extension
        let ext = filename.rsplit('.').next()?.to_lowercase();
        match ext.as_str() {
            "txt" => Some(Self::PlainText),
            "md" | "markdown" => Some(Self::Markdown),
            "pdf" => Some(Self::Pdf),
            "docx" => Some(Self::Docx),
            _ => None,
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::PlainText => "text/plain",
            Self::Markdown => "text/markdown",
            Self::Pdf => "application/pdf",
            Self::Docx => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("extraction error: {0}")]
    Extraction(String),
    #[error("document not found: {0}")]
    NotFound(Uuid),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("document under legal hold: {0}")]
    LegalHold(Uuid),
    #[error("{0}")]
    Other(String),
}
