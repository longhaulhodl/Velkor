mod routes;

use anyhow::Result;
use std::sync::Arc;
use velkor_audit::logger::AuditLogger;
use velkor_documents::store::DocumentStore;
use velkor_memory::service::MemoryService;
use velkor_runtime::react::AgentRuntime;

/// Shared application state available to all axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub memory: Arc<MemoryService>,
    pub audit: AuditLogger,
    pub runtime: Arc<AgentRuntime>,
    pub doc_store: Option<Arc<DocumentStore>>,
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

    // Build tool registry with all Phase 1 built-in tools
    let mut tools = velkor_tools::registry::ToolRegistry::new();

    // Web search tool (auto-detects provider from config: Tavily → Brave → Serper → Perplexity → DuckDuckGo)
    {
        let ws_config = config
            .tools
            .as_ref()
            .and_then(|t| t.web_search.clone())
            .unwrap_or_default();
        if let Some(ws_tool) = velkor_tools::builtin::web_search::WebSearchTool::from_config(&ws_config) {
            tracing::info!("Registered tool: web_search");
            tools.register(Box::new(ws_tool));
        } else {
            // Fallback to DuckDuckGo so web search always works
            tracing::info!("Registered tool: web_search (DuckDuckGo fallback)");
            tools.register(Box::new(velkor_tools::builtin::web_search::WebSearchTool::duckduckgo()));
        }
    }

    // Web fetch tool
    {
        let wf_config = config.tools.as_ref().and_then(|t| t.web_fetch.clone());
        let enabled = wf_config.as_ref().map(|c| c.enabled).unwrap_or(true);
        if enabled {
            let max_len = wf_config.map(|c| c.max_content_length).unwrap_or(50_000);
            tracing::info!("Registered tool: web_fetch (max_content_length={})", max_len);
            tools.register(Box::new(velkor_tools::builtin::web_fetch::WebFetchTool::new(max_len)));
        }
    }

    // Memory tools (store + search)
    {
        let mem_enabled = config
            .tools
            .as_ref()
            .and_then(|t| t.memory.as_ref())
            .map(|m| m.enabled)
            .unwrap_or(true);
        if mem_enabled {
            tracing::info!("Registered tools: memory_store, memory_search");
            tools.register(Box::new(velkor_tools::builtin::memory::MemoryStoreTool::new(Arc::clone(&memory))));
            tools.register(Box::new(velkor_tools::builtin::memory::MemorySearchTool::new(Arc::clone(&memory))));
        }
    }

    // Document tools (read + search) — only if S3 storage is configured
    let doc_store: Option<Arc<DocumentStore>> = {
        let doc_enabled = config
            .tools
            .as_ref()
            .and_then(|t| t.documents.as_ref())
            .map(|d| d.enabled)
            .unwrap_or(true);
        if doc_enabled {
            if let Some(s3_cfg) = config.database.s3.as_ref() {
                let s3_config = aws_sdk_s3::Config::builder()
                    .endpoint_url(&s3_cfg.endpoint)
                    .credentials_provider(aws_sdk_s3::config::Credentials::new(
                        &s3_cfg.access_key,
                        &s3_cfg.secret_key,
                        None,
                        None,
                        "velkor-config",
                    ))
                    .region(aws_sdk_s3::config::Region::new("us-east-1"))
                    .force_path_style(true)
                    .behavior_version_latest()
                    .build();
                let s3_client = aws_sdk_s3::Client::from_conf(s3_config);
                let store = Arc::new(DocumentStore::new(
                    pool.clone(),
                    s3_client,
                    s3_cfg.bucket.clone(),
                    Arc::clone(&embedder),
                ));
                tracing::info!("Registered tools: document_read, document_search");
                tools.register(Box::new(velkor_tools::builtin::documents::DocumentReadTool::new(Arc::clone(&store))));
                tools.register(Box::new(velkor_tools::builtin::documents::DocumentSearchTool::new(Arc::clone(&store))));
                Some(store)
            } else {
                tracing::warn!("Document tools disabled — no S3 storage configured (database.s3)");
                None
            }
        } else {
            None
        }
    };

    tracing::info!(count = tools.len(), names = ?tools.tool_names(), "Tool registry ready");
    let tools = Arc::new(tools);

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
        pool: pool.clone(),
        memory,
        audit,
        runtime,
        doc_store,
    };

    // Start retention background task
    let _retention_handle = velkor_retention::spawn_retention_task(
        pool,
        velkor_retention::RetentionConfig::default(),
    );

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
