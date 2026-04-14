//! Velkor orchestrator — multi-agent coordination and background task execution.
//!
//! Per PRD Section 5.1: Supervisor pattern where a primary agent decides when
//! to handle directly vs delegate to specialized agents.
//!
//! Two major capabilities:
//!
//! 1. **Multi-agent orchestration**: Multiple AgentRuntimes registered by ID.
//!    The supervisor agent has access to DelegationTool which calls sub-agents
//!    and returns their results as tool outputs.
//!
//! 2. **Background tasks**: Long-running agent work spawned on demand. Users
//!    kick off a task and continue chatting. Results are persisted to DB and
//!    notifications pushed via a callback.

pub mod delegation;
pub mod tasks;

use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;
use velkor_config::AgentDefinition;
use velkor_runtime::react::{AgentRuntime, RuntimeConfig};
use velkor_memory::service::MemoryService;
use velkor_memory::MemoryScope;
use velkor_models::router::ModelRouter;
use velkor_audit::logger::AuditLogger;
use velkor_tools::registry::ToolRegistry;

/// The multi-agent orchestrator.
///
/// Holds a registry of named agents, each with its own runtime config,
/// model, system prompt, and tool set. One agent is designated the
/// supervisor (default: "default"). The supervisor gets a DelegationTool
/// injected that can call any other registered agent.
pub struct Orchestrator {
    agents: HashMap<String, Arc<AgentRuntime>>,
    supervisor_id: String,
}

impl Orchestrator {
    /// Create a new orchestrator with the given supervisor ID.
    pub fn new(supervisor_id: impl Into<String>) -> Self {
        Self {
            agents: HashMap::new(),
            supervisor_id: supervisor_id.into(),
        }
    }

    /// Register an agent runtime under the given ID.
    pub fn register_agent(&mut self, id: impl Into<String>, runtime: Arc<AgentRuntime>) {
        let id = id.into();
        info!(agent_id = %id, "Registered agent in orchestrator");
        self.agents.insert(id, runtime);
    }

    /// Get an agent runtime by ID.
    pub fn get_agent(&self, id: &str) -> Option<&Arc<AgentRuntime>> {
        self.agents.get(id)
    }

    /// Get the supervisor agent runtime.
    pub fn supervisor(&self) -> Option<&Arc<AgentRuntime>> {
        self.agents.get(&self.supervisor_id)
    }

    /// Get the supervisor ID.
    pub fn supervisor_id(&self) -> &str {
        &self.supervisor_id
    }

    /// List all registered agent IDs and their descriptions.
    pub fn list_agents(&self) -> Vec<AgentInfo> {
        self.agents
            .iter()
            .map(|(id, runtime)| AgentInfo {
                id: id.clone(),
                model: runtime.config.model.clone(),
                is_supervisor: id == &self.supervisor_id,
            })
            .collect()
    }

    /// Number of registered agents.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }
}

/// Summary info about a registered agent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub model: String,
    pub is_supervisor: bool,
}

/// Shared handle to the orchestrator.
pub type OrchestratorHandle = Arc<Orchestrator>;

// ---------------------------------------------------------------------------
// Builder: construct agent runtimes from YAML definitions
// ---------------------------------------------------------------------------

/// Build an AgentRuntime from an agent YAML definition.
///
/// Each agent can have its own model, system prompt, temperature, and tool
/// subset. They share the model router, memory service, and audit logger.
pub fn build_agent_runtime(
    def: &AgentDefinition,
    platform_name: &str,
    model_router: Arc<ModelRouter>,
    memory: Arc<MemoryService>,
    audit: AuditLogger,
    tools: Arc<ToolRegistry>,
) -> AgentRuntime {
    let mut config = RuntimeConfig::default();
    config.model = def.model.clone();
    config.temperature = Some(def.temperature);

    if let Some(max_tokens) = def.max_context_tokens {
        config.max_tokens = Some(max_tokens as u32);
    }
    if let Some(max_iters) = def.max_tool_calls_per_turn {
        config.max_iterations = max_iters;
    }

    // Memory scope from agent definition
    config.memory_scope = match def.memory_scope.as_deref() {
        Some("shared") => MemoryScope::Shared,
        Some("org") => MemoryScope::Org,
        _ => MemoryScope::Personal,
    };

    // System prompt: use agent-defined or build a default
    let tool_names: Vec<&str> = if def.tools.is_empty() {
        tools.tool_names()
    } else {
        def.tools.iter().map(|s| s.as_str()).collect()
    };

    config.system_prompt = if let Some(ref prompt) = def.system_prompt {
        format!(
            "{prompt}\n\nRuntime: platform={platform} | agent={agent_id} | model={model} | tools={tools}",
            prompt = prompt,
            platform = platform_name,
            agent_id = def.id,
            model = def.model,
            tools = tool_names.join(", "),
        )
    } else {
        format!(
            "You are {name}, a helpful AI assistant.\n\n\
             Runtime: platform={platform} | agent={agent_id} | model={model} | tools={tools}\n\n\
             {description}",
            name = def.name,
            platform = platform_name,
            agent_id = def.id,
            model = def.model,
            tools = tool_names.join(", "),
            description = def.description.as_deref().unwrap_or("Be concise, accurate, and helpful."),
        )
    };

    config.skill_self_improve = def.self_improve;

    AgentRuntime::new(config, model_router, memory, audit, tools)
}
