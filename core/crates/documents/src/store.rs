use crate::{
    Document, DocumentError, DocumentFormat, DocumentSearchResult, NewDocument,
    extractors,
};
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use tracing::{debug, info};
use uuid::Uuid;
use velkor_memory::EmbeddingProvider;
use std::sync::Arc;

/// Document store backed by S3 (files) + PostgreSQL (metadata + search).
///
/// Files are stored in S3-compatible storage (MinIO for self-hosted).
/// Only metadata, extracted text, and embeddings live in PostgreSQL.
pub struct DocumentStore {
    pool: PgPool,
    s3: S3Client,
    bucket: String,
    embedder: Arc<dyn EmbeddingProvider>,
}

impl DocumentStore {
    pub fn new(
        pool: PgPool,
        s3: S3Client,
        bucket: String,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            pool,
            s3,
            bucket,
            embedder,
        }
    }

    /// Upload a document: store file in S3, extract text, generate embedding,
    /// insert metadata into PostgreSQL.
    pub async fn upload(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        filename: &str,
        data: Vec<u8>,
    ) -> Result<Document, DocumentError> {
        let file_size = data.len() as i64;

        // Detect format and extract text
        let format = DocumentFormat::detect(None, filename);
        let mime_type = format.map(|f| f.mime_type().to_string());

        let content_text = match format {
            Some(fmt) => match extractors::extract_text(&data, fmt) {
                Ok(text) => Some(text),
                Err(DocumentError::UnsupportedFormat(_)) => {
                    // Store without text — still searchable by filename
                    debug!(filename, "Format not yet supported for extraction, storing without text");
                    None
                }
                Err(e) => return Err(e),
            },
            None => None,
        };

        // Generate embedding from extracted text
        let embedding = if let Some(ref text) = content_text {
            // Truncate to avoid huge embeddings — use first ~8K chars
            let truncated = if text.len() > 8000 { &text[..8000] } else { text };
            match self.embedder.embed(truncated).await {
                Ok(emb) => Some(emb),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to embed document, storing without vector");
                    None
                }
            }
        } else {
            None
        };

        // Upload to S3
        let storage_key = format!(
            "documents/{workspace_id}/{}/{filename}",
            Uuid::new_v4()
        );
        self.put_s3(&storage_key, data, mime_type.as_deref())
            .await?;

        // Insert metadata into PostgreSQL
        let new_doc = NewDocument {
            workspace_id,
            user_id,
            filename: filename.to_string(),
            mime_type,
            file_size,
            storage_key,
            content_text,
            content_embedding: embedding,
            metadata: serde_json::json!({}),
        };

        let doc = self.insert_metadata(&new_doc).await?;

        info!(id = %doc.id, filename = %doc.filename, "Document uploaded");
        Ok(doc)
    }

    /// Download a document's file content from S3.
    pub async fn download(&self, id: Uuid) -> Result<(Document, Vec<u8>), DocumentError> {
        let doc = self.get(id).await?.ok_or(DocumentError::NotFound(id))?;

        if doc.legal_hold {
            // Legal hold doesn't prevent download, but we log it
            debug!(id = %id, "Downloading document under legal hold");
        }

        let data = self.get_s3(&doc.storage_key).await?;
        Ok((doc, data))
    }

    /// Get document metadata by ID.
    pub async fn get(&self, id: Uuid) -> Result<Option<Document>, DocumentError> {
        let row = sqlx::query_as::<_, DocumentRow>(
            r#"
            SELECT id, workspace_id, user_id, filename, mime_type, file_size,
                   storage_key, content_text, metadata, created_at, updated_at,
                   is_deleted, legal_hold
            FROM documents
            WHERE id = $1 AND NOT is_deleted
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_document()))
    }

    /// List documents in a workspace.
    pub async fn list(
        &self,
        workspace_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Document>, DocumentError> {
        let rows = sqlx::query_as::<_, DocumentRow>(
            r#"
            SELECT id, workspace_id, user_id, filename, mime_type, file_size,
                   storage_key, content_text, metadata, created_at, updated_at,
                   is_deleted, legal_hold
            FROM documents
            WHERE workspace_id = $1 AND NOT is_deleted
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(workspace_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_document()).collect())
    }

    /// Hybrid search across documents (FTS + vector), same RRF approach as memory.
    pub async fn search(
        &self,
        query: &str,
        workspace_id: Uuid,
        limit: usize,
    ) -> Result<Vec<DocumentSearchResult>, DocumentError> {
        let embedding = match self.embedder.embed(query).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to embed search query, falling back to FTS-only");
                None
            }
        };

        let limit_i64 = limit as i64;

        if let Some(emb) = embedding {
            let vec = Vector::from(emb);
            let rows = sqlx::query_as::<_, SearchRow>(
                r#"
                WITH fts_results AS (
                    SELECT id, filename, mime_type, workspace_id,
                           left(content_text, 200) AS snippet,
                           ts_rank(search_vector, plainto_tsquery('english', $1)) AS fts_score,
                           created_at
                    FROM documents
                    WHERE search_vector @@ plainto_tsquery('english', $1)
                      AND workspace_id = $2 AND NOT is_deleted
                    ORDER BY fts_score DESC
                    LIMIT 20
                ),
                vector_results AS (
                    SELECT id, filename, mime_type, workspace_id,
                           left(content_text, 200) AS snippet,
                           1 - (content_embedding <=> $3::vector) AS vec_score,
                           created_at
                    FROM documents
                    WHERE workspace_id = $2 AND NOT is_deleted
                      AND content_embedding IS NOT NULL
                    ORDER BY content_embedding <=> $3::vector
                    LIMIT 20
                ),
                combined AS (
                    SELECT
                        COALESCE(f.id, v.id) AS id,
                        COALESCE(f.filename, v.filename) AS filename,
                        COALESCE(f.mime_type, v.mime_type) AS mime_type,
                        COALESCE(f.workspace_id, v.workspace_id) AS workspace_id,
                        COALESCE(f.snippet, v.snippet) AS snippet,
                        COALESCE(f.created_at, v.created_at) AS created_at,
                        COALESCE(1.0 / (60 + ROW_NUMBER() OVER (ORDER BY f.fts_score DESC NULLS LAST)), 0) AS fts_rrf,
                        COALESCE(1.0 / (60 + ROW_NUMBER() OVER (ORDER BY v.vec_score DESC NULLS LAST)), 0) AS vec_rrf
                    FROM fts_results f
                    FULL OUTER JOIN vector_results v ON f.id = v.id
                )
                SELECT id, filename, mime_type, workspace_id, snippet, created_at,
                       (fts_rrf + vec_rrf) AS combined_score
                FROM combined
                ORDER BY combined_score DESC
                LIMIT $4
                "#,
            )
            .bind(query)
            .bind(workspace_id)
            .bind(vec)
            .bind(limit_i64)
            .fetch_all(&self.pool)
            .await?;

            Ok(rows.into_iter().map(|r| r.into_result()).collect())
        } else {
            // FTS-only fallback
            let rows = sqlx::query_as::<_, FtsSearchRow>(
                r#"
                SELECT id, filename, mime_type, workspace_id,
                       left(content_text, 200) AS snippet,
                       ts_rank(search_vector, plainto_tsquery('english', $1)) AS fts_score,
                       created_at
                FROM documents
                WHERE search_vector @@ plainto_tsquery('english', $1)
                  AND workspace_id = $2 AND NOT is_deleted
                ORDER BY fts_score DESC
                LIMIT $3
                "#,
            )
            .bind(query)
            .bind(workspace_id)
            .bind(limit_i64)
            .fetch_all(&self.pool)
            .await?;

            Ok(rows.into_iter().map(|r| r.into_result()).collect())
        }
    }

    /// Soft-delete a document. Respects legal holds.
    pub async fn delete(&self, id: Uuid) -> Result<(), DocumentError> {
        // Check legal hold first
        let hold = sqlx::query_scalar::<_, bool>(
            "SELECT legal_hold FROM documents WHERE id = $1 AND NOT is_deleted",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match hold {
            None => return Err(DocumentError::NotFound(id)),
            Some(true) => return Err(DocumentError::LegalHold(id)),
            Some(false) => {}
        }

        sqlx::query(
            r#"
            UPDATE documents
            SET is_deleted = TRUE, deleted_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        debug!(%id, "Soft-deleted document");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // S3 operations
    // -----------------------------------------------------------------------

    async fn put_s3(
        &self,
        key: &str,
        data: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<(), DocumentError> {
        let mut req = self
            .s3
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data));

        if let Some(ct) = content_type {
            req = req.content_type(ct);
        }

        req.send()
            .await
            .map_err(|e| DocumentError::Storage(format!("S3 put failed: {e}")))?;

        debug!(key, "Uploaded to S3");
        Ok(())
    }

    async fn get_s3(&self, key: &str) -> Result<Vec<u8>, DocumentError> {
        let resp = self
            .s3
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| DocumentError::Storage(format!("S3 get failed: {e}")))?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| DocumentError::Storage(format!("S3 read body failed: {e}")))?
            .into_bytes();

        Ok(bytes.to_vec())
    }

    // -----------------------------------------------------------------------
    // Postgres metadata insert
    // -----------------------------------------------------------------------

    async fn insert_metadata(&self, doc: &NewDocument) -> Result<Document, DocumentError> {
        let embedding = doc
            .content_embedding
            .as_ref()
            .map(|e| Vector::from(e.clone()));

        let row = sqlx::query_as::<_, DocumentRow>(
            r#"
            INSERT INTO documents
                (workspace_id, user_id, filename, mime_type, file_size,
                 storage_key, content_text, content_embedding, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, workspace_id, user_id, filename, mime_type, file_size,
                      storage_key, content_text, metadata, created_at, updated_at,
                      is_deleted, legal_hold
            "#,
        )
        .bind(doc.workspace_id)
        .bind(doc.user_id)
        .bind(&doc.filename)
        .bind(&doc.mime_type)
        .bind(doc.file_size)
        .bind(&doc.storage_key)
        .bind(&doc.content_text)
        .bind(embedding)
        .bind(&doc.metadata)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into_document())
    }
}

// ---------------------------------------------------------------------------
// Internal row types for sqlx mapping
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct DocumentRow {
    id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
    filename: String,
    mime_type: Option<String>,
    file_size: Option<i64>,
    storage_key: String,
    content_text: Option<String>,
    metadata: JsonValue,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    is_deleted: bool,
    legal_hold: bool,
}

impl DocumentRow {
    fn into_document(self) -> Document {
        Document {
            id: self.id,
            workspace_id: self.workspace_id,
            user_id: self.user_id,
            filename: self.filename,
            mime_type: self.mime_type,
            file_size: self.file_size,
            storage_key: self.storage_key,
            content_text: self.content_text,
            metadata: self.metadata,
            created_at: self.created_at,
            updated_at: self.updated_at,
            is_deleted: self.is_deleted,
            legal_hold: self.legal_hold,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SearchRow {
    id: Uuid,
    filename: String,
    mime_type: Option<String>,
    workspace_id: Uuid,
    snippet: Option<String>,
    created_at: DateTime<Utc>,
    combined_score: f64,
}

impl SearchRow {
    fn into_result(self) -> DocumentSearchResult {
        DocumentSearchResult {
            id: self.id,
            filename: self.filename,
            mime_type: self.mime_type,
            workspace_id: self.workspace_id,
            snippet: self.snippet.unwrap_or_default(),
            score: self.combined_score,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct FtsSearchRow {
    id: Uuid,
    filename: String,
    mime_type: Option<String>,
    workspace_id: Uuid,
    snippet: Option<String>,
    fts_score: f32,
    created_at: DateTime<Utc>,
}

impl FtsSearchRow {
    fn into_result(self) -> DocumentSearchResult {
        DocumentSearchResult {
            id: self.id,
            filename: self.filename,
            mime_type: self.mime_type,
            workspace_id: self.workspace_id,
            snippet: self.snippet.unwrap_or_default(),
            score: self.fts_score as f64,
            created_at: self.created_at,
        }
    }
}
