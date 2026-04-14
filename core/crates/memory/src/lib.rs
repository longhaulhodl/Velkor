pub mod postgres;
pub mod service;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryScope {
    Personal,
    Shared,
    Org,
}

impl MemoryScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Shared => "shared",
            Self::Org => "org",
        }
    }
}

impl std::fmt::Display for MemoryScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryCategory {
    Fact,
    Preference,
    Project,
    Procedure,
    Relationship,
}

impl MemoryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Preference => "preference",
            Self::Project => "project",
            Self::Procedure => "procedure",
            Self::Relationship => "relationship",
        }
    }
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A memory record as stored in the backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub org_id: Option<Uuid>,
    pub scope: MemoryScope,
    pub category: Option<MemoryCategory>,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub source_conversation_id: Option<Uuid>,
    pub confidence: f32,
    /// Importance score: 1 (trivial) to 10 (critical). Default 5.
    pub importance: i16,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a new memory (no id/timestamps — those are generated).
#[derive(Debug, Clone)]
pub struct NewMemory {
    pub user_id: Uuid,
    pub org_id: Option<Uuid>,
    pub scope: MemoryScope,
    pub category: Option<MemoryCategory>,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub source_conversation_id: Option<Uuid>,
    pub confidence: f32,
    /// Importance score: 1 (trivial) to 10 (critical).
    pub importance: i16,
}

/// A search result with relevance score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResult {
    pub id: Uuid,
    pub content: String,
    pub scope: MemoryScope,
    pub category: Option<MemoryCategory>,
    pub confidence: f32,
    pub importance: i16,
    /// Combined RRF score from hybrid search (higher = more relevant).
    pub score: f64,
    pub created_at: DateTime<Utc>,
}

/// A recalled conversation with its summary and messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRecall {
    pub id: Uuid,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub messages: Vec<RecalledMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecalledMessage {
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// Search result for conversation history search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryResult {
    pub conversation_id: Uuid,
    pub conversation_title: Option<String>,
    pub message_content: String,
    pub message_role: String,
    pub score: f64,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// MemoryBackend trait
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("memory not found: {0}")]
    NotFound(Uuid),
    #[error("{0}")]
    Other(String),
}

#[async_trait]
pub trait MemoryBackend: Send + Sync {
    /// Store a new memory. Returns the generated ID.
    async fn store(&self, memory: &NewMemory) -> Result<Uuid, MemoryError>;

    /// Hybrid search: FTS + vector similarity, merged with Reciprocal Rank Fusion.
    /// If `embedding` is None, falls back to FTS-only.
    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        scope: MemoryScope,
        user_id: Uuid,
        limit: usize,
    ) -> Result<Vec<MemoryResult>, MemoryError>;

    /// Retrieve a specific memory by ID.
    async fn get(&self, id: Uuid) -> Result<Option<MemoryRecord>, MemoryError>;

    /// Update the content (and optionally embedding) of an existing memory.
    async fn update(
        &self,
        id: Uuid,
        content: &str,
        embedding: Option<&[f32]>,
    ) -> Result<(), MemoryError>;

    /// Soft-delete a memory.
    async fn delete(&self, id: Uuid) -> Result<(), MemoryError>;

    /// Recall a past conversation by ID, including its messages.
    async fn recall(&self, conversation_id: Uuid) -> Result<Option<ConversationRecall>, MemoryError>;

    /// Search across conversation history (messages table) using FTS + vector.
    async fn search_history(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        user_id: Uuid,
        limit: usize,
    ) -> Result<Vec<HistoryResult>, MemoryError>;

    /// Find memories similar to the given embedding (cosine similarity).
    /// Used for pre-storage dedup: if a near-duplicate exists, update instead of add.
    /// Returns memories with similarity >= threshold, sorted by similarity desc.
    async fn find_similar(
        &self,
        embedding: &[f32],
        user_id: Uuid,
        scope: MemoryScope,
        threshold: f64,
        limit: usize,
    ) -> Result<Vec<MemoryResult>, MemoryError>;

    /// Retrieve high-importance memories (importance >= min_importance) for a user.
    /// These are "core memories" that should be injected into the system prompt.
    async fn get_core_memories(
        &self,
        user_id: Uuid,
        min_importance: i16,
        limit: usize,
    ) -> Result<Vec<MemoryResult>, MemoryError>;

    /// Hard-delete all memories for a user (GDPR erasure). Returns count deleted.
    async fn purge_user(&self, user_id: Uuid) -> Result<u64, MemoryError>;

    /// Health check — can we reach the backend?
    async fn health(&self) -> Result<bool, MemoryError>;
}

// ---------------------------------------------------------------------------
// EmbeddingProvider trait
// ---------------------------------------------------------------------------

/// Generates vector embeddings from text. Implemented by the models crate
/// or any external embedding service.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for the given text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError>;
}
