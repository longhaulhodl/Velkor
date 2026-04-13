use crate::{
    ConversationRecall, EmbeddingProvider, HistoryResult, MemoryBackend, MemoryCategory,
    MemoryError, MemoryRecord, MemoryResult, MemoryScope, NewMemory,
};
use std::sync::Arc;
use tracing::{debug, warn};
use uuid::Uuid;

/// High-level memory service that agents interact with.
///
/// Wraps a `MemoryBackend` and an `EmbeddingProvider` so that callers never
/// need to generate embeddings manually — `store()`, `update()`, and `search()`
/// all handle embedding automatically.
pub struct MemoryService {
    backend: Arc<dyn MemoryBackend>,
    embedder: Arc<dyn EmbeddingProvider>,
}

impl MemoryService {
    pub fn new(backend: Arc<dyn MemoryBackend>, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self { backend, embedder }
    }

    /// Store a new memory. Automatically generates the embedding from `content`.
    pub async fn store(
        &self,
        user_id: Uuid,
        content: &str,
        scope: MemoryScope,
        category: Option<MemoryCategory>,
        source_conversation_id: Option<Uuid>,
    ) -> Result<Uuid, MemoryError> {
        let embedding = match self.embedder.embed(content).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                // Embedding failure is non-fatal — store without embedding,
                // FTS still works. Log and continue.
                warn!(error = %e, "Failed to generate embedding, storing without vector");
                None
            }
        };

        let memory = NewMemory {
            user_id,
            org_id: None,
            scope,
            category,
            content: content.to_string(),
            embedding,
            source_conversation_id,
            confidence: 1.0,
        };

        let id = self.backend.store(&memory).await?;
        debug!(%id, %scope, "Memory stored");
        Ok(id)
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

    /// Hard-delete all memories for a user (GDPR right to erasure).
    pub async fn purge_user(&self, user_id: Uuid) -> Result<u64, MemoryError> {
        self.backend.purge_user(user_id).await
    }

    /// Health check.
    pub async fn health(&self) -> Result<bool, MemoryError> {
        self.backend.health().await
    }
}
