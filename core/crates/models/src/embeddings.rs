//! Embedding provider that speaks the OpenAI `/v1/embeddings` API format.
//!
//! This covers OpenAI, OpenRouter, Ollama, Together, and any other endpoint
//! that implements the same wire format.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use velkor_memory::MemoryError;

/// A real embedding provider that calls an OpenAI-compatible embeddings API.
pub struct EmbeddingClient {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    dimensions: Option<u32>,
}

impl EmbeddingClient {
    pub fn new(
        api_key: String,
        base_url: String,
        model: String,
        dimensions: Option<u32>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url,
            model,
            dimensions,
        }
    }

    /// Build from the config's `embedding_model` string (e.g. "openai/text-embedding-3-small")
    /// and the providers map.
    pub fn from_config(
        embedding_model: &str,
        dimensions: u32,
        providers: &std::collections::HashMap<String, velkor_config::ProviderConfig>,
    ) -> Option<Self> {
        let (provider_name, model_name) = if let Some(pos) = embedding_model.find('/') {
            (&embedding_model[..pos], &embedding_model[pos + 1..])
        } else {
            // No prefix — try openai, then openrouter
            if providers.contains_key("openai") {
                ("openai", embedding_model)
            } else if providers.contains_key("openrouter") {
                ("openrouter", embedding_model)
            } else {
                warn!("No provider prefix in embedding_model and no openai/openrouter configured");
                return None;
            }
        };

        let provider = providers.get(provider_name)?;

        let (base_url, api_key) = match provider_name {
            "openai" => (
                provider
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                provider.api_key.clone().unwrap_or_default(),
            ),
            "openrouter" => (
                provider
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string()),
                provider.api_key.clone().unwrap_or_default(),
            ),
            "ollama" => (
                provider
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434/v1".to_string()),
                String::new(), // Ollama doesn't need auth
            ),
            other => {
                // Generic OpenAI-compatible: must have base_url
                let base = provider.base_url.clone().unwrap_or_else(|| {
                    warn!(provider = other, "No base_url for custom embedding provider");
                    String::new()
                });
                (base, provider.api_key.clone().unwrap_or_default())
            }
        };

        if base_url.is_empty() {
            warn!(provider = provider_name, "Empty base_url for embedding provider");
            return None;
        }

        // text-embedding-3-small supports dimensions param; others may not
        let dims = if model_name.contains("text-embedding-3") {
            Some(dimensions)
        } else {
            None
        };

        debug!(
            provider = provider_name,
            model = model_name,
            base_url = %base_url,
            "Embedding provider configured"
        );

        Some(Self::new(api_key, base_url, model_name.to_string(), dims))
    }

    fn embeddings_url(&self) -> String {
        format!("{}/embeddings", self.base_url)
    }
}

#[async_trait::async_trait]
impl velkor_memory::EmbeddingProvider for EmbeddingClient {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError> {
        let body = EmbeddingRequest {
            model: &self.model,
            input: text,
            dimensions: self.dimensions,
        };

        let mut req = self
            .client
            .post(self.embeddings_url())
            .header("Content-Type", "application/json");

        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let resp = req.json(&body).send().await.map_err(|e| {
            MemoryError::Other(format!("embedding HTTP error: {e}"))
        })?;

        let status = resp.status();
        if !status.is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            return Err(MemoryError::Other(format!(
                "embedding API error ({status}): {err_body}"
            )));
        }

        let response: EmbeddingResponse = resp.json().await.map_err(|e| {
            MemoryError::Other(format!("embedding response parse error: {e}"))
        })?;

        let embedding = response
            .data
            .into_iter()
            .next()
            .ok_or_else(|| MemoryError::Other("empty embedding response".to_string()))?
            .embedding;

        debug!(dims = embedding.len(), "Generated embedding");
        Ok(embedding)
    }
}

// ---------------------------------------------------------------------------
// Wire types for the OpenAI embeddings API
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}
