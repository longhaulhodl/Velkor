use crate::{
    ConversationRecall, HistoryResult, MemoryBackend, MemoryCategory, MemoryError, MemoryRecord,
    MemoryResult, MemoryScope, NewMemory, RecalledMessage,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pgvector::Vector;
use sqlx::PgPool;
use tracing::debug;
use uuid::Uuid;

/// PostgreSQL + pgvector implementation of the MemoryBackend trait.
/// Uses hybrid FTS + vector search with Reciprocal Rank Fusion scoring.
pub struct PostgresMemory {
    pool: PgPool,
}

impl PostgresMemory {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// ---------------------------------------------------------------------------
// Helper: parse scope/category strings from DB rows
// ---------------------------------------------------------------------------

fn parse_scope(s: &str) -> MemoryScope {
    match s {
        "personal" => MemoryScope::Personal,
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

// ---------------------------------------------------------------------------
// MemoryBackend implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl MemoryBackend for PostgresMemory {
    async fn store(&self, memory: &NewMemory) -> Result<Uuid, MemoryError> {
        let embedding = memory.embedding.as_ref().map(|e| Vector::from(e.clone()));
        let scope_str = memory.scope.as_str();
        let category_str = memory.category.as_ref().map(|c| c.as_str().to_string());

        let row = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO memories (user_id, org_id, scope, category, content, embedding,
                                  source_conversation_id, confidence, importance)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id
            "#,
        )
        .bind(memory.user_id)
        .bind(memory.org_id)
        .bind(scope_str)
        .bind(&category_str)
        .bind(&memory.content)
        .bind(embedding)
        .bind(memory.source_conversation_id)
        .bind(memory.confidence)
        .bind(memory.importance)
        .fetch_one(&self.pool)
        .await?;

        debug!(id = %row, scope = scope_str, importance = memory.importance, "Stored new memory");
        Ok(row)
    }

    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        scope: MemoryScope,
        user_id: Uuid,
        limit: usize,
    ) -> Result<Vec<MemoryResult>, MemoryError> {
        let scope_str = scope.as_str();
        let limit_i64 = limit as i64;

        // If we have an embedding, do full hybrid search with RRF.
        // If not, fall back to FTS-only.
        if let Some(emb) = embedding {
            let vec = Vector::from(emb.to_vec());
            let rows = sqlx::query_as::<_, HybridRow>(
                r#"
                WITH fts_results AS (
                    SELECT id, content, scope, category, confidence, importance, created_at,
                           ts_rank(search_vector, plainto_tsquery('english', $1)) AS fts_score
                    FROM memories
                    WHERE search_vector @@ plainto_tsquery('english', $1)
                      AND user_id = $2 AND scope = $3 AND NOT is_deleted
                    ORDER BY fts_score DESC
                    LIMIT 20
                ),
                vector_results AS (
                    SELECT id, content, scope, category, confidence, importance, created_at,
                           1 - (embedding <=> $4::vector) AS vec_score
                    FROM memories
                    WHERE user_id = $2 AND scope = $3 AND NOT is_deleted
                      AND embedding IS NOT NULL
                    ORDER BY embedding <=> $4::vector
                    LIMIT 20
                ),
                combined AS (
                    SELECT
                        COALESCE(f.id, v.id) AS id,
                        COALESCE(f.content, v.content) AS content,
                        COALESCE(f.scope, v.scope) AS scope,
                        COALESCE(f.category, v.category) AS category,
                        COALESCE(f.confidence, v.confidence) AS confidence,
                        COALESCE(f.importance, v.importance) AS importance,
                        COALESCE(f.created_at, v.created_at) AS created_at,
                        COALESCE((1.0 / (60 + ROW_NUMBER() OVER (ORDER BY f.fts_score DESC NULLS LAST)))::float8, 0) AS fts_rrf,
                        COALESCE((1.0 / (60 + ROW_NUMBER() OVER (ORDER BY v.vec_score DESC NULLS LAST)))::float8, 0) AS vec_rrf
                    FROM fts_results f
                    FULL OUTER JOIN vector_results v ON f.id = v.id
                )
                SELECT id, content, scope, category, confidence, importance, created_at,
                       (fts_rrf + vec_rrf)::float8 AS combined_score
                FROM combined
                ORDER BY combined_score DESC
                LIMIT $5
                "#,
            )
            .bind(query)
            .bind(user_id)
            .bind(scope_str)
            .bind(vec)
            .bind(limit_i64)
            .fetch_all(&self.pool)
            .await?;

            Ok(rows.into_iter().map(|r| r.into_result()).collect())
        } else {
            // FTS-only fallback
            let rows = sqlx::query_as::<_, FtsRow>(
                r#"
                SELECT id, content, scope, category, confidence, importance, created_at,
                       ts_rank(search_vector, plainto_tsquery('english', $1)) AS fts_score
                FROM memories
                WHERE search_vector @@ plainto_tsquery('english', $1)
                  AND user_id = $2 AND scope = $3 AND NOT is_deleted
                ORDER BY fts_score DESC
                LIMIT $4
                "#,
            )
            .bind(query)
            .bind(user_id)
            .bind(scope_str)
            .bind(limit_i64)
            .fetch_all(&self.pool)
            .await?;

            Ok(rows.into_iter().map(|r| r.into_result()).collect())
        }
    }

    async fn get(&self, id: Uuid) -> Result<Option<MemoryRecord>, MemoryError> {
        let row = sqlx::query_as::<_, MemoryRow>(
            r#"
            SELECT id, user_id, org_id, scope, category, content,
                   embedding, source_conversation_id, confidence, importance,
                   created_at, updated_at
            FROM memories
            WHERE id = $1 AND NOT is_deleted
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_record()))
    }

    async fn update(
        &self,
        id: Uuid,
        content: &str,
        embedding: Option<&[f32]>,
    ) -> Result<(), MemoryError> {
        let vec = embedding.map(|e| Vector::from(e.to_vec()));

        let result = sqlx::query(
            r#"
            UPDATE memories
            SET content = $1,
                embedding = COALESCE($2, embedding),
                updated_at = now()
            WHERE id = $3 AND NOT is_deleted
            "#,
        )
        .bind(content)
        .bind(vec)
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MemoryError::NotFound(id));
        }

        debug!(%id, "Updated memory");
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), MemoryError> {
        let result = sqlx::query(
            r#"
            UPDATE memories
            SET is_deleted = TRUE, deleted_at = now()
            WHERE id = $1 AND NOT is_deleted
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MemoryError::NotFound(id));
        }

        debug!(%id, "Soft-deleted memory");
        Ok(())
    }

    async fn recall(
        &self,
        conversation_id: Uuid,
    ) -> Result<Option<ConversationRecall>, MemoryError> {
        // Fetch conversation metadata.
        // Our conversations table is partitioned, so we query without
        // constraining started_at — Postgres scans all partitions.
        let conv = sqlx::query_as::<_, ConversationRow>(
            r#"
            SELECT id, title, summary, started_at, ended_at
            FROM conversations
            WHERE id = $1 AND NOT is_deleted
            "#,
        )
        .bind(conversation_id)
        .fetch_optional(&self.pool)
        .await?;

        let conv = match conv {
            Some(c) => c,
            None => return Ok(None),
        };

        // Fetch messages for this conversation.
        let messages = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT role, content, created_at
            FROM messages
            WHERE conversation_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(Some(ConversationRecall {
            id: conv.id,
            title: conv.title,
            summary: conv.summary,
            started_at: conv.started_at,
            ended_at: conv.ended_at,
            messages: messages
                .into_iter()
                .map(|m| RecalledMessage {
                    role: m.role,
                    content: m.content,
                    created_at: m.created_at,
                })
                .collect(),
        }))
    }

    async fn search_history(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        user_id: Uuid,
        limit: usize,
    ) -> Result<Vec<HistoryResult>, MemoryError> {
        // FTS search across the messages table, joining conversations for metadata.
        // Vector search on messages would require embeddings per-message which we
        // don't store in Phase 1. We use conversation summary_embedding later.
        let limit_i64 = limit as i64;

        let rows = sqlx::query_as::<_, HistoryRow>(
            r#"
            SELECT m.conversation_id,
                   c.title AS conversation_title,
                   m.content AS message_content,
                   m.role AS message_role,
                   ts_rank(m.search_vector, plainto_tsquery('english', $1)) AS score,
                   m.created_at
            FROM messages m
            JOIN conversations c ON c.id = m.conversation_id
            WHERE m.search_vector @@ plainto_tsquery('english', $1)
              AND c.user_id = $2
              AND NOT c.is_deleted
            ORDER BY score DESC
            LIMIT $3
            "#,
        )
        .bind(query)
        .bind(user_id)
        .bind(limit_i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| HistoryResult {
                conversation_id: r.conversation_id,
                conversation_title: r.conversation_title,
                message_content: r.message_content,
                message_role: r.message_role,
                score: r.score as f64,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn find_similar(
        &self,
        embedding: &[f32],
        user_id: Uuid,
        scope: MemoryScope,
        threshold: f64,
        limit: usize,
    ) -> Result<Vec<MemoryResult>, MemoryError> {
        let vec = Vector::from(embedding.to_vec());
        let scope_str = scope.as_str();
        let limit_i64 = limit as i64;

        let rows = sqlx::query_as::<_, SimilarRow>(
            r#"
            SELECT id, content, scope, category, confidence, importance, created_at,
                   (1 - (embedding <=> $1::vector))::float8 AS similarity
            FROM memories
            WHERE user_id = $2 AND scope = $3 AND NOT is_deleted
              AND embedding IS NOT NULL
              AND (1 - (embedding <=> $1::vector)) >= $4
            ORDER BY similarity DESC
            LIMIT $5
            "#,
        )
        .bind(&vec)
        .bind(user_id)
        .bind(scope_str)
        .bind(threshold)
        .bind(limit_i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| MemoryResult {
                id: r.id,
                content: r.content,
                scope: parse_scope(&r.scope),
                category: r.category.as_deref().and_then(parse_category),
                confidence: r.confidence,
                importance: r.importance,
                score: r.similarity,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn get_core_memories(
        &self,
        user_id: Uuid,
        min_importance: i16,
        limit: usize,
    ) -> Result<Vec<MemoryResult>, MemoryError> {
        let limit_i64 = limit as i64;

        let rows = sqlx::query_as::<_, CoreRow>(
            r#"
            SELECT id, content, scope, category, confidence, importance, created_at
            FROM memories
            WHERE user_id = $1 AND NOT is_deleted
              AND importance >= $2
            ORDER BY importance DESC, updated_at DESC
            LIMIT $3
            "#,
        )
        .bind(user_id)
        .bind(min_importance)
        .bind(limit_i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| MemoryResult {
                id: r.id,
                content: r.content,
                scope: parse_scope(&r.scope),
                category: r.category.as_deref().and_then(parse_category),
                confidence: r.confidence,
                importance: r.importance,
                score: r.importance as f64 / 10.0,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn purge_user(&self, user_id: Uuid) -> Result<u64, MemoryError> {
        // Hard delete — for GDPR right-to-erasure.
        let result = sqlx::query(
            r#"
            DELETE FROM memories WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        let count = result.rows_affected();
        debug!(%user_id, count, "Purged all memories for user");
        Ok(count)
    }

    async fn health(&self) -> Result<bool, MemoryError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Internal row types for sqlx mapping
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct HybridRow {
    id: Uuid,
    content: String,
    scope: String,
    category: Option<String>,
    confidence: f32,
    importance: i16,
    created_at: DateTime<Utc>,
    combined_score: f64,
}

impl HybridRow {
    fn into_result(self) -> MemoryResult {
        MemoryResult {
            id: self.id,
            content: self.content,
            scope: parse_scope(&self.scope),
            category: self.category.as_deref().and_then(parse_category),
            confidence: self.confidence,
            importance: self.importance,
            score: self.combined_score,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct FtsRow {
    id: Uuid,
    content: String,
    scope: String,
    category: Option<String>,
    confidence: f32,
    importance: i16,
    created_at: DateTime<Utc>,
    fts_score: f32,
}

impl FtsRow {
    fn into_result(self) -> MemoryResult {
        MemoryResult {
            id: self.id,
            content: self.content,
            scope: parse_scope(&self.scope),
            category: self.category.as_deref().and_then(parse_category),
            confidence: self.confidence,
            importance: self.importance,
            score: self.fts_score as f64,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SimilarRow {
    id: Uuid,
    content: String,
    scope: String,
    category: Option<String>,
    confidence: f32,
    importance: i16,
    created_at: DateTime<Utc>,
    similarity: f64,
}

#[derive(sqlx::FromRow)]
struct CoreRow {
    id: Uuid,
    content: String,
    scope: String,
    category: Option<String>,
    confidence: f32,
    importance: i16,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: Uuid,
    user_id: Uuid,
    org_id: Option<Uuid>,
    scope: String,
    category: Option<String>,
    content: String,
    embedding: Option<Vector>,
    source_conversation_id: Option<Uuid>,
    confidence: f32,
    importance: i16,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl MemoryRow {
    fn into_record(self) -> MemoryRecord {
        MemoryRecord {
            id: self.id,
            user_id: self.user_id,
            org_id: self.org_id,
            scope: parse_scope(&self.scope),
            category: self.category.as_deref().and_then(parse_category),
            content: self.content,
            embedding: self.embedding.map(|v| v.to_vec()),
            source_conversation_id: self.source_conversation_id,
            confidence: self.confidence,
            importance: self.importance,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ConversationRow {
    id: Uuid,
    title: Option<String>,
    summary: Option<String>,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    role: String,
    content: String,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct HistoryRow {
    conversation_id: Uuid,
    conversation_title: Option<String>,
    message_content: String,
    message_role: String,
    score: f32,
    created_at: DateTime<Utc>,
}
