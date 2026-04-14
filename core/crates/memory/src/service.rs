use crate::{
    ConversationRecall, EmbeddingProvider, HistoryResult, MemoryBackend, MemoryCategory,
    MemoryError, MemoryRecord, MemoryResult, MemoryScope, NewMemory,
};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Minimum importance threshold — memories below this are rejected.
const MIN_IMPORTANCE: i16 = 3;

/// Cosine similarity threshold for deduplication.
/// If a new memory is >= this similar to an existing one, update instead of add.
const DEDUP_SIMILARITY_THRESHOLD: f64 = 0.92;

/// Minimum importance to qualify as a "core memory" (always in prompt).
const CORE_MEMORY_IMPORTANCE: i16 = 8;

/// Max core memories injected into the system prompt.
const CORE_MEMORY_LIMIT: usize = 15;

/// High-level memory service that agents interact with.
///
/// Wraps a `MemoryBackend` and an `EmbeddingProvider` so that callers never
/// need to generate embeddings manually — `store()`, `update()`, and `search()`
/// all handle embedding automatically.
///
/// Includes quality gates:
/// - Importance threshold: rejects memories scored below MIN_IMPORTANCE
/// - Deduplication: detects near-duplicate memories and updates instead of adding
/// - Core memories: high-importance memories auto-injected into prompts
pub struct MemoryService {
    backend: Arc<dyn MemoryBackend>,
    embedder: Arc<dyn EmbeddingProvider>,
}

/// Result of a store operation — either a new memory was created,
/// an existing one was updated (dedup), or the memory was rejected.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum StoreResult {
    /// New memory created with this ID.
    Created(#[serde(rename = "id")] Uuid),
    /// Existing memory updated (dedup hit). Contains the existing memory's ID.
    Updated(#[serde(rename = "id")] Uuid),
    /// Memory rejected (importance too low).
    Rejected { reason: String },
}

impl MemoryService {
    pub fn new(backend: Arc<dyn MemoryBackend>, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self { backend, embedder }
    }

    /// Store a new memory with quality gates:
    /// 1. Reject if importance < MIN_IMPORTANCE
    /// 2. Generate embedding
    /// 3. Check for near-duplicates via cosine similarity
    /// 4. If duplicate found, update existing memory instead
    /// 5. Otherwise, create new memory
    pub async fn store(
        &self,
        user_id: Uuid,
        content: &str,
        scope: MemoryScope,
        category: Option<MemoryCategory>,
        source_conversation_id: Option<Uuid>,
        importance: i16,
    ) -> Result<StoreResult, MemoryError> {
        // Gate 1: importance threshold
        if importance < MIN_IMPORTANCE {
            debug!(importance, min = MIN_IMPORTANCE, "Memory rejected: importance too low");
            return Ok(StoreResult::Rejected {
                reason: format!(
                    "Importance {} is below minimum threshold {}. Only store durable, \
                     referenceable facts — not ephemeral queries or conversation chatter.",
                    importance, MIN_IMPORTANCE
                ),
            });
        }

        // Generate embedding
        let embedding = match self.embedder.embed(content).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                warn!(error = %e, "Failed to generate embedding, storing without vector");
                None
            }
        };

        // Gate 2: deduplication via cosine similarity
        if let Some(ref emb) = embedding {
            match self
                .backend
                .find_similar(emb, user_id, scope, DEDUP_SIMILARITY_THRESHOLD, 1)
                .await
            {
                Ok(similar) => {
                    if let Some(existing) = similar.first() {
                        // Near-duplicate found — update the existing memory
                        info!(
                            existing_id = %existing.id,
                            similarity = existing.score,
                            "Dedup: updating existing memory instead of creating duplicate"
                        );
                        self.backend
                            .update(existing.id, content, Some(emb))
                            .await?;
                        return Ok(StoreResult::Updated(existing.id));
                    }
                }
                Err(e) => {
                    // Dedup check failed — log and proceed with store
                    warn!(error = %e, "Dedup check failed, storing anyway");
                }
            }
        }

        // Create new memory
        let memory = NewMemory {
            user_id,
            org_id: None,
            scope,
            category,
            content: content.to_string(),
            embedding,
            source_conversation_id,
            confidence: 1.0,
            importance,
        };

        let id = self.backend.store(&memory).await?;
        debug!(%id, %scope, importance, "Memory stored");
        Ok(StoreResult::Created(id))
    }

    /// Hybrid search: embeds the query, then runs FTS + vector search with RRF.
    pub async fn search(
        &self,
        query: &str,
        scope: MemoryScope,
        user_id: Uuid,
        limit: usize,
    ) -> Result<Vec<MemoryResult>, MemoryError> {
        let embedding = match self.embedder.embed(query).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                warn!(error = %e, "Failed to embed query, falling back to FTS-only");
                None
            }
        };

        self.backend
            .search(query, embedding.as_deref(), scope, user_id, limit)
            .await
    }

    /// Retrieve a specific memory by ID.
    pub async fn get(&self, id: Uuid) -> Result<Option<MemoryRecord>, MemoryError> {
        self.backend.get(id).await
    }

    /// Update a memory's content. Automatically re-generates the embedding.
    pub async fn update(&self, id: Uuid, content: &str) -> Result<(), MemoryError> {
        let embedding = match self.embedder.embed(content).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                warn!(error = %e, "Failed to re-embed on update, keeping old embedding");
                None
            }
        };

        self.backend
            .update(id, content, embedding.as_deref())
            .await
    }

    /// Soft-delete a memory (retention-aware).
    pub async fn delete(&self, id: Uuid) -> Result<(), MemoryError> {
        self.backend.delete(id).await
    }

    /// Recall a past conversation with its messages.
    pub async fn recall(
        &self,
        conversation_id: Uuid,
    ) -> Result<Option<ConversationRecall>, MemoryError> {
        self.backend.recall(conversation_id).await
    }

    /// Search across all conversation history.
    pub async fn search_history(
        &self,
        query: &str,
        user_id: Uuid,
        limit: usize,
    ) -> Result<Vec<HistoryResult>, MemoryError> {
        let embedding = match self.embedder.embed(query).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                warn!(error = %e, "Failed to embed history query, falling back to FTS-only");
                None
            }
        };

        self.backend
            .search_history(query, embedding.as_deref(), user_id, limit)
            .await
    }

    /// Retrieve core memories (high-importance) for system prompt injection.
    pub async fn get_core_memories(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<MemoryResult>, MemoryError> {
        self.backend
            .get_core_memories(user_id, CORE_MEMORY_IMPORTANCE, CORE_MEMORY_LIMIT)
            .await
    }

    /// Hard-delete all memories for a user (GDPR right to erasure).
    pub async fn purge_user(&self, user_id: Uuid) -> Result<u64, MemoryError> {
        self.backend.purge_user(user_id).await
    }

    /// Health check.
    pub async fn health(&self) -> Result<bool, MemoryError> {
        self.backend.health().await
    }
}
