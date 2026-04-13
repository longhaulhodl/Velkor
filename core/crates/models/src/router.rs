use crate::{ChatRequest, LlmProvider, LlmResponse, ProviderError, StreamResult};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Retry configuration
// ---------------------------------------------------------------------------

/// Controls retry behavior for transient errors.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts per model (not counting the first try).
    pub max_retries: u32,
    /// Initial backoff delay. Doubles on each retry (exponential).
    pub initial_backoff: Duration,
    /// Ceiling on backoff delay.
    pub max_backoff: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
        }
    }
}

/// Returns true for errors that are worth retrying on the *same* model.
fn is_retryable(err: &ProviderError) -> bool {
    match err {
        ProviderError::RateLimited { .. } => true,
        ProviderError::Http(e) => e.is_timeout() || e.is_connect(),
        ProviderError::Api { status, .. } => matches!(status, 500 | 502 | 503 | 504),
        _ => false,
    }
}

/// Compute the backoff delay for a given attempt.
fn backoff_delay(attempt: u32, config: &RetryConfig, err: &ProviderError) -> Duration {
    // If the provider told us how long to wait, respect that
    if let ProviderError::RateLimited {
        retry_after_ms: Some(ms),
    } = err
    {
        return Duration::from_millis(*ms).min(config.max_backoff);
    }

    let delay = config
        .initial_backoff
        .saturating_mul(2u32.saturating_pow(attempt));
    delay.min(config.max_backoff)
}

// ---------------------------------------------------------------------------
// Cost tracker
// ---------------------------------------------------------------------------

/// Tracks token usage and cost across all requests.
#[derive(Debug, Default)]
pub struct CostTracker {
    total_input_tokens: AtomicU64,
    total_output_tokens: AtomicU64,
    /// Stored as micro-USD for atomic precision.
    total_cost_micro_usd: AtomicU64,
}

impl CostTracker {
    pub fn record(&self, input_tokens: u32, output_tokens: u32, cost_usd: f64) {
        self.total_input_tokens
            .fetch_add(input_tokens as u64, Ordering::Relaxed);
        self.total_output_tokens
            .fetch_add(output_tokens as u64, Ordering::Relaxed);
        let micro = (cost_usd * 1_000_000.0) as u64;
        self.total_cost_micro_usd.fetch_add(micro, Ordering::Relaxed);
    }

    pub fn total_input_tokens(&self) -> u64 {
        self.total_input_tokens.load(Ordering::Relaxed)
    }

    pub fn total_output_tokens(&self) -> u64 {
        self.total_output_tokens.load(Ordering::Relaxed)
    }

    pub fn total_cost_usd(&self) -> f64 {
        self.total_cost_micro_usd.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }
}

// ---------------------------------------------------------------------------
// Model router
// ---------------------------------------------------------------------------

/// Routes model requests to the appropriate provider, retries on transient
/// errors, walks fallback chains, and tracks costs.
pub struct ModelRouter {
    providers: HashMap<String, Box<dyn LlmProvider>>,
    fallback_chain: Vec<String>,
    pub cost_tracker: Arc<CostTracker>,
    pub retry_config: RetryConfig,
}

impl ModelRouter {
    pub fn new(fallback_chain: Vec<String>) -> Self {
        Self {
            providers: HashMap::new(),
            fallback_chain,
            cost_tracker: Arc::new(CostTracker::default()),
            retry_config: RetryConfig::default(),
        }
    }

    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    pub fn add_provider(&mut self, key: impl Into<String>, provider: Box<dyn LlmProvider>) {
        self.providers.insert(key.into(), provider);
    }

    fn resolve_model<'a>(
        &'a self,
        model: &'a str,
    ) -> Option<(&'a str, &'a str, &'a dyn LlmProvider)> {
        if let Some((provider_key, model_id)) = model.split_once('/') {
            if let Some(provider) = self.providers.get(provider_key) {
                return Some((provider_key, model_id, provider.as_ref()));
            }
        }

        for (key, provider) in &self.providers {
            if provider.supports_model(model) {
                return Some((key, model, provider.as_ref()));
            }
        }

        None
    }

    /// Non-streaming request with retry + fallback.
    pub async fn chat(&self, request: &ChatRequest<'_>) -> Result<LlmResponse, ProviderError> {
        // Try the requested model with retries
        match self.try_chat_with_retry(request.model, request).await {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                warn!(
                    model = request.model,
                    error = %e,
                    "Primary model exhausted retries, trying fallback chain"
                );
            }
        }

        // Walk the fallback chain (each with retries)
        for fallback_model in &self.fallback_chain {
            if fallback_model == request.model {
                continue;
            }
            match self.try_chat_with_retry(fallback_model, request).await {
                Ok(resp) => {
                    info!(model = fallback_model, "Fallback model succeeded");
                    return Ok(resp);
                }
                Err(e) => {
                    warn!(
                        model = fallback_model,
                        error = %e,
                        "Fallback model exhausted retries"
                    );
                }
            }
        }

        Err(ProviderError::Other(
            "All models in fallback chain failed".into(),
        ))
    }

    /// Streaming request with retry + fallback.
    /// Note: retries only apply to connection-level failures. Once streaming
    /// begins, the stream is returned and errors are delivered as StreamChunk::Error.
    pub async fn chat_stream(
        &self,
        request: &ChatRequest<'_>,
    ) -> Result<StreamResult, ProviderError> {
        match self.try_stream_with_retry(request.model, request).await {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                warn!(
                    model = request.model,
                    error = %e,
                    "Primary model stream exhausted retries, trying fallback chain"
                );
            }
        }

        for fallback_model in &self.fallback_chain {
            if fallback_model == request.model {
                continue;
            }
            match self.try_stream_with_retry(fallback_model, request).await {
                Ok(stream) => {
                    info!(model = fallback_model, "Fallback stream succeeded");
                    return Ok(stream);
                }
                Err(e) => {
                    warn!(
                        model = fallback_model,
                        error = %e,
                        "Fallback stream exhausted retries"
                    );
                }
            }
        }

        Err(ProviderError::Other(
            "All models in fallback chain failed".into(),
        ))
    }

    pub fn cost_per_token(&self, model: &str) -> (f64, f64) {
        if let Some((_, _, provider)) = self.resolve_model(model) {
            provider.cost_per_token(model)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.keys().map(|k| k.as_str()).collect()
    }

    // -----------------------------------------------------------------------
    // Internal: single-model attempt with retry
    // -----------------------------------------------------------------------

    async fn try_chat_with_retry(
        &self,
        model: &str,
        original_request: &ChatRequest<'_>,
    ) -> Result<LlmResponse, ProviderError> {
        let (_, model_id, provider) = self
            .resolve_model(model)
            .ok_or_else(|| ProviderError::UnsupportedModel(model.to_string()))?;

        let request = ChatRequest {
            model: model_id,
            ..*original_request
        };

        let mut last_err = None;

        for attempt in 0..=self.retry_config.max_retries {
            match provider.chat(&request).await {
                Ok(resp) => {
                    let (ip, op) = provider.cost_per_token(model);
                    let cost = resp.usage.cost(ip, op);
                    self.cost_tracker
                        .record(resp.usage.input_tokens, resp.usage.output_tokens, cost);
                    return Ok(resp);
                }
                Err(e) => {
                    if attempt < self.retry_config.max_retries && is_retryable(&e) {
                        let delay = backoff_delay(attempt, &self.retry_config, &e);
                        warn!(
                            model,
                            attempt = attempt + 1,
                            max = self.retry_config.max_retries,
                            delay_ms = delay.as_millis() as u64,
                            error = %e,
                            "Retryable error, backing off"
                        );
                        tokio::time::sleep(delay).await;
                        last_err = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| ProviderError::Other("retry loop exited unexpectedly".into())))
    }

    async fn try_stream_with_retry(
        &self,
        model: &str,
        original_request: &ChatRequest<'_>,
    ) -> Result<StreamResult, ProviderError> {
        let (_, model_id, provider) = self
            .resolve_model(model)
            .ok_or_else(|| ProviderError::UnsupportedModel(model.to_string()))?;

        let request = ChatRequest {
            model: model_id,
            ..*original_request
        };

        let mut last_err = None;

        for attempt in 0..=self.retry_config.max_retries {
            match provider.chat_stream(&request).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    if attempt < self.retry_config.max_retries && is_retryable(&e) {
                        let delay = backoff_delay(attempt, &self.retry_config, &e);
                        warn!(
                            model,
                            attempt = attempt + 1,
                            max = self.retry_config.max_retries,
                            delay_ms = delay.as_millis() as u64,
                            error = %e,
                            "Stream retryable error, backing off"
                        );
                        tokio::time::sleep(delay).await;
                        last_err = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| ProviderError::Other("retry loop exited unexpectedly".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_tracker() {
        let tracker = CostTracker::default();
        tracker.record(100, 50, 0.001);
        tracker.record(200, 100, 0.002);
        assert_eq!(tracker.total_input_tokens(), 300);
        assert_eq!(tracker.total_output_tokens(), 150);
        assert!((tracker.total_cost_usd() - 0.003).abs() < 1e-9);
    }

    #[test]
    fn test_is_retryable() {
        assert!(is_retryable(&ProviderError::RateLimited {
            retry_after_ms: Some(1000)
        }));
        assert!(is_retryable(&ProviderError::Api {
            status: 500,
            message: "internal".into()
        }));
        assert!(is_retryable(&ProviderError::Api {
            status: 503,
            message: "unavailable".into()
        }));
        assert!(!is_retryable(&ProviderError::Api {
            status: 400,
            message: "bad request".into()
        }));
        assert!(!is_retryable(&ProviderError::UnsupportedModel(
            "x".into()
        )));
    }

    #[test]
    fn test_backoff_delay_exponential() {
        let config = RetryConfig {
            max_retries: 5,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
        };
        let err = ProviderError::Api {
            status: 500,
            message: "".into(),
        };

        assert_eq!(backoff_delay(0, &config, &err), Duration::from_millis(100));
        assert_eq!(backoff_delay(1, &config, &err), Duration::from_millis(200));
        assert_eq!(backoff_delay(2, &config, &err), Duration::from_millis(400));
        assert_eq!(backoff_delay(3, &config, &err), Duration::from_millis(800));
    }

    #[test]
    fn test_backoff_delay_capped() {
        let config = RetryConfig {
            max_retries: 10,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(5),
        };
        let err = ProviderError::Api {
            status: 500,
            message: "".into(),
        };

        // 1 * 2^5 = 32s, but capped at 5s
        assert_eq!(backoff_delay(5, &config, &err), Duration::from_secs(5));
    }

    #[test]
    fn test_backoff_respects_retry_after() {
        let config = RetryConfig::default();
        let err = ProviderError::RateLimited {
            retry_after_ms: Some(2000),
        };

        assert_eq!(backoff_delay(0, &config, &err), Duration::from_secs(2));
    }
}
