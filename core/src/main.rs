mod routes;
mod schedule_tools;

use anyhow::Result;
use std::sync::Arc;
use velkor_audit::logger::AuditLogger;
use velkor_documents::store::DocumentStore;
use velkor_memory::service::MemoryService;
use velkor_runtime::react::AgentRuntime;
use velkor_tools::builtin::skills::SkillStoreHandle;

/// Shared application state available to all axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub memory: Arc<MemoryService>,
    pub audit: AuditLogger,
    pub runtime: Arc<AgentRuntime>,
    pub doc_store: Option<Arc<DocumentStore>>,
    pub skill_store: SkillStoreHandle,
    pub retention_status: velkor_retention::RetentionStatusHandle,
    pub scheduler_status: velkor_scheduler::SchedulerStatusHandle,
    pub orchestrator: Option<velkor_orchestrator::OrchestratorHandle>,
    pub task_notifier: velkor_orchestrator::tasks::TaskNotifier,
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

    // Delegation tools — lazy orchestrator handle set after orchestrator is built
    let lazy_orch = velkor_orchestrator::delegation::lazy_orchestrator();
    {
        tracing::info!("Registered tools: delegate_to_agent, delegate_parallel");
        tools.register(Box::new(
            velkor_orchestrator::delegation::DelegationTool::new(
                lazy_orch.clone(),
                pool.clone(),
            ),
        ));
        tools.register(Box::new(
            velkor_orchestrator::delegation::ParallelDelegationTool::new(
                lazy_orch.clone(),
                pool.clone(),
            ),
        ));
    }

    // Skills tools (installable SKILL.md + learned DB skills)
    let skill_store: SkillStoreHandle = {
        let skills_cfg = config
            .tools
            .as_ref()
            .and_then(|t| t.skills.clone())
            .unwrap_or_default();

        let skill_dirs: Vec<std::path::PathBuf> = if skills_cfg.directories.is_empty() {
            vec![std::path::PathBuf::from("skills")]
        } else {
            skills_cfg.directories.iter().map(std::path::PathBuf::from).collect()
        };

        let mut store = velkor_skills::store::SkillStore::new(pool.clone(), skill_dirs);
        let loaded = store.load_installable_skills();
        tracing::info!(count = loaded, "Loaded installable skills from disk");

        let handle: SkillStoreHandle = Arc::new(tokio::sync::RwLock::new(store));

        if skills_cfg.enabled {
            tracing::info!("Registered tools: skill_list, skill_view, skill_manage");
            tools.register(Box::new(
                velkor_tools::builtin::skills::SkillListTool::new(Arc::clone(&handle)),
            ));
            tools.register(Box::new(
                velkor_tools::builtin::skills::SkillViewTool::new(Arc::clone(&handle)),
            ));
            tools.register(Box::new(
                velkor_tools::builtin::skills::SkillManageTool::new(Arc::clone(&handle)),
            ));
        }

        handle
    };

    // Schedule management tools (cron CRUD from chat)
    {
        let sched_enabled = config
            .scheduling
            .as_ref()
            .map(|s| s.enabled)
            .unwrap_or(true);
        if sched_enabled {
            tracing::info!("Registered tools: schedule_list, schedule_create, schedule_update, schedule_delete");
            tools.register(Box::new(schedule_tools::ScheduleListTool::new(pool.clone())));
            tools.register(Box::new(schedule_tools::ScheduleCreateTool::new(pool.clone())));
            tools.register(Box::new(schedule_tools::ScheduleUpdateTool::new(pool.clone())));
            tools.register(Box::new(schedule_tools::ScheduleDeleteTool::new(pool.clone())));
        }
    }

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

    // Wire skill review config
    {
        let skills_cfg = config
            .tools
            .as_ref()
            .and_then(|t| t.skills.as_ref());
        runtime_config.skill_self_improve = skills_cfg.map(|s| s.self_improve).unwrap_or(true);
        runtime_config.skill_review_threshold = skills_cfg.map(|s| s.review_threshold).unwrap_or(10);
    }
    let tool_names: Vec<&str> = tools.tool_names();
    let mut system_prompt = format!(
        "You are {name}, a helpful AI assistant.\n\n\
         Runtime: platform={name} | model={model} | tools={tool_list}\n\n\
         You have access to tools and should use them proactively. \
         When the user asks you to search the web, use the web_search tool. \
         When asked about documents, use document_search or document_read. \
         Be concise, accurate, and helpful. If you don't know something, say so \
         rather than making up information. \
         When using tool results, present the information faithfully — \
         do not fabricate details beyond what the tool returned.",
        name = config.platform.name,
        model = runtime_config.model,
        tool_list = tool_names.join(", "),
    );

    // Load agent definitions early so we can include them in the system prompt
    let agents_dir = std::path::Path::new("agents");
    let agent_configs: Vec<velkor_config::AgentConfig> = if agents_dir.is_dir() {
        velkor_config::load_all_agents(agents_dir).unwrap_or_default()
    } else {
        vec![]
    };

    // If we have specialized agents, add delegation instructions to the system prompt
    if !agent_configs.is_empty() {
        system_prompt.push_str("\n\n## Multi-Agent Delegation\n\n\
            You are the supervisor agent. You can delegate tasks to specialized agents \
            using the `delegate_to_agent` tool (single agent) or `delegate_parallel` tool \
            (multiple agents simultaneously). Decide whether to handle a request yourself \
            or delegate based on the task complexity and agent expertise.\n\n\
            Available agents:\n");
        for ac in &agent_configs {
            system_prompt.push_str(&format!(
                "- **{}** (model: {}): {}\n",
                ac.agent.id,
                ac.agent.model,
                ac.agent.description.as_deref().unwrap_or("General-purpose agent"),
            ));
        }
    }

    // Append skills index (progressive disclosure tier 1: names + descriptions)
    {
        let store = skill_store.read().await;
        if let Some(skills_block) = velkor_skills::index::build_skills_prompt(&store).await {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&skills_block);
        }
    }
    runtime_config.system_prompt = system_prompt;

    let runtime = Arc::new(
        AgentRuntime::new(
            runtime_config,
            model_router,
            Arc::clone(&memory),
            audit.clone(),
            tools,
        )
        .with_skill_store(Arc::clone(&skill_store)),
    );

    // Start retention background task with config from YAML
    let retention_config = {
        let mut rc = velkor_retention::RetentionConfig::default();
        if let Some(ref ret) = config.retention {
            if let Some(days) = ret.default_conversation_days {
                rc.default_retention_days = days as i64;
            }
            if !ret.auto_purge {
                rc.interval_secs = 86400 * 365; // effectively off
            }
            rc.hard_delete = false; // always soft-delete for safety
        }
        rc
    };
    let retention_status = velkor_retention::new_status_handle(&retention_config);
    let _retention_handle = velkor_retention::spawn_retention_task(
        pool.clone(),
        retention_config,
        Arc::clone(&retention_status),
    );

    // Start scheduler heartbeat background task
    let scheduler_config = {
        let sched = config.scheduling.as_ref();
        velkor_scheduler::SchedulerConfig {
            enabled: sched.map(|s| s.enabled).unwrap_or(true),
            heartbeat_secs: sched.map(|s| s.heartbeat_interval_seconds).unwrap_or(60),
            timezone: sched.and_then(|s| s.timezone.clone()),
        }
    };
    let scheduler_status = velkor_scheduler::new_status_handle(&scheduler_config);
    let _scheduler_handle = velkor_scheduler::spawn_scheduler_task(
        pool.clone(),
        scheduler_config,
        Arc::clone(&runtime),
        Arc::clone(&scheduler_status),
    );

    // Build orchestrator from the agent definitions loaded earlier
    let orchestrator: Option<velkor_orchestrator::OrchestratorHandle> = {
        let mut orch = velkor_orchestrator::Orchestrator::new("default");
        // Always register the default runtime
        orch.register_agent("default", Arc::clone(&runtime));

        if !agent_configs.is_empty() {
            for ac in &agent_configs {
                let agent_runtime = velkor_orchestrator::build_agent_runtime(
                    &ac.agent,
                    &config.platform.name,
                    Arc::clone(&runtime.model_router),
                    Arc::clone(&runtime.memory),
                    runtime.audit.clone(),
                    Arc::clone(&runtime.tools),
                );
                let agent_runtime = Arc::new(
                    agent_runtime.with_skill_store(Arc::clone(&skill_store))
                );
                orch.register_agent(&ac.agent.id, agent_runtime);
            }
            tracing::info!(
                agents = orch.agent_count(),
                "Orchestrator initialized with agents from agents/ directory"
            );
        } else {
            tracing::info!("No agent definitions — single-agent mode");
        }

        let orch_handle = Arc::new(orch);

        // Set the lazy orchestrator handle so delegation tools become active
        let _ = lazy_orch.set(Arc::clone(&orch_handle));
        tracing::info!("Delegation tools bound to orchestrator");

        if !agent_configs.is_empty() { Some(orch_handle) } else { None }
    };

    // Task notification channel (for WebSocket push when background tasks complete)
    let task_notifier = velkor_orchestrator::tasks::new_notifier();

    let state = AppState {
        pool: pool.clone(),
        memory,
        audit,
        runtime,
        doc_store,
        skill_store,
        retention_status,
        scheduler_status,
        orchestrator,
        task_notifier,
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
