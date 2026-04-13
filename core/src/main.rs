mod routes;

use anyhow::Result;
use std::sync::Arc;
use velkor_audit::logger::AuditLogger;
use velkor_memory::service::MemoryService;
use velkor_runtime::react::AgentRuntime;

/// Shared application state available to all axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub memory: Arc<MemoryService>,
    pub audit: AuditLogger,
    pub runtime: Arc<AgentRuntime>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    tracing::info!("Velkor agent platform starting");

    // Load config
    let config_path = std::env::var("VELKOR_CONFIG")
        .unwrap_or_else(|_| "config.yaml".to_string());
    let config = velkor_config::load_platform_config(std::path::Path::new(&config_path))?;

    // Connect to Postgres
    let pool = velkor_db::create_pg_pool(&config.database.postgres_url).await?;

    // Initialize core services
    let audit = AuditLogger::new(pool.clone());

    // Build embedding provider from config (falls back to NoopEmbedder if unconfigured)
    let embedder: Arc<dyn velkor_memory::EmbeddingProvider> = {
        let mem_cfg = config.memory.as_ref();
        let model = mem_cfg
            .map(|m| m.embedding_model.as_str())
            .unwrap_or("openai/text-embedding-3-small");
        let dims = mem_cfg.map(|m| m.embedding_dimensions).unwrap_or(1536);

        match velkor_models::embeddings::EmbeddingClient::from_config(
            model,
            dims,
            &config.providers,
        ) {
            Some(client) => {
                tracing::info!(model = model, dims = dims, "Real embedding provider configured");
                Arc::new(client)
            }
            None => {
                tracing::warn!("No embedding provider configured — vector search disabled, FTS-only");
                Arc::new(NoopEmbedder)
            }
        }
    };

    let memory = Arc::new(MemoryService::new(
        Arc::new(velkor_memory::postgres::PostgresMemory::new(pool.clone())),
        Arc::clone(&embedder),
    ));

    // Build model router and register providers from config
    let mut model_router = velkor_models::router::ModelRouter::new(
        config
            .routing
            .as_ref()
            .map(|r| r.fallback_chain.clone())
            .unwrap_or_default(),
    );

    for (name, provider_cfg) in &config.providers {
        let api_key = provider_cfg.api_key.clone().unwrap_or_default();
        let provider: Box<dyn velkor_models::LlmProvider> = match name.as_str() {
            "anthropic" => {
                Box::new(velkor_models::anthropic::AnthropicProvider::new(
                    api_key,
                    provider_cfg.base_url.clone(),
                ))
            }
            "openai" => {
                Box::new(velkor_models::openai_compat::OpenAICompatProvider::openai(api_key))
            }
            "ollama" => {
                Box::new(velkor_models::openai_compat::OpenAICompatProvider::ollama(
                    provider_cfg.base_url.clone(),
                ))
            }
            "openrouter" => {
                Box::new(velkor_models::openai_compat::OpenAICompatProvider::openrouter(api_key))
            }
            other => {
                Box::new(velkor_models::openai_compat::OpenAICompatProvider::custom(
                    other.to_string(),
                    api_key,
                    provider_cfg.base_url.clone().unwrap_or_default(),
                ))
            }
        };
        tracing::info!(provider = name, "Registered LLM provider");
        model_router.add_provider(name, provider);
    }

    let model_router = Arc::new(model_router);

    // Build tool registry
    let tools = Arc::new(velkor_tools::registry::ToolRegistry::new());

    // Build agent runtime — use the default model from the first configured provider
    let default_model = config
        .routing
        .as_ref()
        .and_then(|r| r.fallback_chain.first().cloned())
        .or_else(|| {
            config.providers.values().find_map(|p| p.default_model.clone())
        })
        .unwrap_or_else(|| "anthropic/claude-sonnet-4-20250514".to_string());

    tracing::info!(model = %default_model, "Default model for agent runtime");

    let mut runtime_config = velkor_runtime::react::RuntimeConfig::default();
    runtime_config.model = default_model;

    let runtime = Arc::new(AgentRuntime::new(
        runtime_config,
        model_router,
        Arc::clone(&memory),
        audit.clone(),
        tools,
    ));

    let state = AppState {
        pool,
        memory,
        audit,
        runtime,
    };

    // Build router
    let app = routes::internal_router().with_state(state);

    // Start server
    let core_port: u16 = std::env::var("CORE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3001);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{core_port}")).await?;
    tracing::info!("Velkor core API listening on port {core_port}");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Placeholder embedding provider for startup. In production, this would
/// be wired to the model router's embedding endpoint.
struct NoopEmbedder;

#[async_trait::async_trait]
impl velkor_memory::EmbeddingProvider for NoopEmbedder {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, velkor_memory::MemoryError> {
        // Returns empty — FTS still works, vector search is skipped
        Err(velkor_memory::MemoryError::Other(
            "No embedding provider configured".to_string(),
        ))
    }
}
