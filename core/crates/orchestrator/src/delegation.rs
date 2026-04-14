//! DelegationTool — allows the supervisor agent to delegate tasks to sub-agents.
//!
//! Two operations:
//! - `delegate`: Run a single sub-agent with a task prompt, return its result
//! - `delegate_parallel`: Run multiple sub-agents concurrently via tokio::join!
//!
//! The supervisor sees all available agents in its system prompt and decides
//! when to delegate vs handle directly. Sub-agent results are returned as
//! tool results that the supervisor synthesizes into a final response.

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;
use velkor_runtime::context::ConversationContext;
use velkor_runtime::react::AgentRuntime;
use velkor_tools::{Tool, ToolContext, ToolError, ToolResult};

use crate::OrchestratorHandle;

/// Lazy handle that allows delegation tools to be registered before the
/// orchestrator is fully built. Call `set()` once the orchestrator is ready.
pub type LazyOrchestrator = Arc<tokio::sync::OnceCell<OrchestratorHandle>>;

/// Create a new lazy orchestrator handle.
pub fn lazy_orchestrator() -> LazyOrchestrator {
    Arc::new(tokio::sync::OnceCell::new())
}

// ---------------------------------------------------------------------------
// DelegationTool: delegate to a single sub-agent
// ---------------------------------------------------------------------------

/// Tool that delegates a task to another agent and returns the result.
///
/// Input schema:
/// ```json
/// {
///   "agent_id": "researcher",
///   "task": "Find the latest news about Rust programming language"
/// }
/// ```
pub struct DelegationTool {
    orchestrator: LazyOrchestrator,
    pool: sqlx::PgPool,
}

impl DelegationTool {
    pub fn new(orchestrator: LazyOrchestrator, pool: sqlx::PgPool) -> Self {
        Self { orchestrator, pool }
    }
}

#[async_trait]
impl Tool for DelegationTool {
    fn name(&self) -> &str {
        "delegate_to_agent"
    }

    fn description(&self) -> &str {
        "Delegate a task to a specialized agent. The agent will execute the task and return its response. Use this when a task is better suited to a specific agent's expertise."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "The ID of the agent to delegate to"
                },
                "task": {
                    "type": "string",
                    "description": "The task description / prompt to send to the agent"
                }
            },
            "required": ["agent_id", "task"]
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let orch = self.orchestrator.get().ok_or_else(|| {
            ToolError::ExecutionFailed("Orchestrator not initialized yet".into())
        })?;

        let agent_id = input["agent_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("missing agent_id".into()))?;
        let task = input["task"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("missing task".into()))?;

        let runtime = orch
            .get_agent(agent_id)
            .ok_or_else(|| {
                let available: Vec<_> = orch
                    .list_agents()
                    .iter()
                    .map(|a| a.id.clone())
                    .collect();
                ToolError::InvalidInput(format!(
                    "Unknown agent '{}'. Available agents: {}",
                    agent_id,
                    available.join(", ")
                ))
            })?;

        info!(
            agent_id = agent_id,
            task_len = task.len(),
            "Delegating task to sub-agent"
        );

        let result = run_sub_agent(
            runtime,
            &self.pool,
            ctx.user_id,
            agent_id,
            task,
            Some(ctx.conversation_id),
        )
        .await;

        match result {
            Ok(response) => {
                info!(
                    agent_id = agent_id,
                    iterations = response.iterations,
                    tokens = response.usage.input_tokens + response.usage.output_tokens,
                    "Sub-agent delegation completed"
                );
                Ok(ToolResult::success(format!(
                    "[Agent '{}' response ({} iterations, {} tokens)]\n\n{}",
                    agent_id,
                    response.iterations,
                    response.usage.input_tokens + response.usage.output_tokens,
                    response.content,
                )))
            }
            Err(e) => {
                warn!(agent_id = agent_id, error = %e, "Sub-agent delegation failed");
                Ok(ToolResult::error(format!(
                    "Agent '{}' failed: {}",
                    agent_id, e
                )))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ParallelDelegationTool: delegate to multiple agents concurrently
// ---------------------------------------------------------------------------

/// Tool that delegates tasks to multiple agents in parallel.
///
/// Input schema:
/// ```json
/// {
///   "delegations": [
///     { "agent_id": "researcher", "task": "Find X" },
///     { "agent_id": "writer", "task": "Draft Y" }
///   ]
/// }
/// ```
pub struct ParallelDelegationTool {
    orchestrator: LazyOrchestrator,
    pool: sqlx::PgPool,
}

impl ParallelDelegationTool {
    pub fn new(orchestrator: LazyOrchestrator, pool: sqlx::PgPool) -> Self {
        Self { orchestrator, pool }
    }
}

#[async_trait]
impl Tool for ParallelDelegationTool {
    fn name(&self) -> &str {
        "delegate_parallel"
    }

    fn description(&self) -> &str {
        "Delegate tasks to multiple agents simultaneously. All agents run in parallel and results are returned together. Use this when you need independent work from multiple specialists."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "delegations": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "agent_id": { "type": "string", "description": "Agent ID" },
                            "task": { "type": "string", "description": "Task prompt" }
                        },
                        "required": ["agent_id", "task"]
                    },
                    "description": "List of agent + task pairs to run in parallel"
                }
            },
            "required": ["delegations"]
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let orch = self.orchestrator.get().ok_or_else(|| {
            ToolError::ExecutionFailed("Orchestrator not initialized yet".into())
        })?;

        let delegations = input["delegations"]
            .as_array()
            .ok_or_else(|| ToolError::InvalidInput("missing delegations array".into()))?;

        if delegations.is_empty() {
            return Err(ToolError::InvalidInput("delegations array is empty".into()));
        }

        if delegations.len() > 5 {
            return Err(ToolError::InvalidInput(
                "Maximum 5 parallel delegations allowed".into(),
            ));
        }

        info!(
            count = delegations.len(),
            "Spawning parallel agent delegations"
        );

        // Collect agent references and tasks
        let mut handles = Vec::new();

        for d in delegations {
            let agent_id = d["agent_id"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidInput("delegation missing agent_id".into()))?
                .to_string();
            let task = d["task"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidInput("delegation missing task".into()))?
                .to_string();

            let runtime = orch
                .get_agent(&agent_id)
                .ok_or_else(|| {
                    ToolError::InvalidInput(format!("Unknown agent '{}'", agent_id))
                })?
                .clone();

            let pool = self.pool.clone();
            let user_id = ctx.user_id;
            let conv_id = ctx.conversation_id;

            handles.push(tokio::spawn(async move {
                let result =
                    run_sub_agent(&runtime, &pool, user_id, &agent_id, &task, Some(conv_id))
                        .await;
                (agent_id, result)
            }));
        }

        // Await all
        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok((agent_id, Ok(response))) => {
                    results.push(format!(
                        "--- Agent '{}' ({} iterations, {} tokens) ---\n{}",
                        agent_id,
                        response.iterations,
                        response.usage.input_tokens + response.usage.output_tokens,
                        response.content,
                    ));
                }
                Ok((agent_id, Err(e))) => {
                    results.push(format!("--- Agent '{}' FAILED ---\n{}", agent_id, e));
                }
                Err(e) => {
                    results.push(format!("--- Agent task panicked ---\n{}", e));
                }
            }
        }

        Ok(ToolResult::success(results.join("\n\n")))
    }
}

// ---------------------------------------------------------------------------
// Shared: run a sub-agent
// ---------------------------------------------------------------------------

/// Run a sub-agent with a new conversation context.
/// Creates a conversation record, persists messages, returns the response.
async fn run_sub_agent(
    runtime: &AgentRuntime,
    pool: &sqlx::PgPool,
    user_id: Uuid,
    agent_id: &str,
    task: &str,
    parent_conversation_id: Option<Uuid>,
) -> Result<velkor_runtime::react::AgentResponse, anyhow::Error> {
    let conversation_id = Uuid::new_v4();

    // Create conversation record for the sub-agent's work
    let title = format!(
        "[Delegated] {}",
        if task.len() > 60 {
            format!("{}...", &task[..60])
        } else {
            task.to_string()
        }
    );

    let _ = sqlx::query(
        "INSERT INTO conversations (id, user_id, agent_id, title, started_at) \
         VALUES ($1, $2, $3, $4, now())",
    )
    .bind(conversation_id)
    .bind(user_id)
    .bind(agent_id)
    .bind(&title)
    .execute(pool)
    .await;

    // Persist the delegated prompt as a user message
    let _ = sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) \
         VALUES ($1, $2, 'user', $3, now())",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(task)
    .execute(pool)
    .await;

    // Build context and run
    let mut context = ConversationContext::new(conversation_id, user_id, agent_id);

    // If there's a parent conversation, note it in the sub-agent's context
    let full_prompt = if let Some(parent_id) = parent_conversation_id {
        format!(
            "[This task was delegated from conversation {}]\n\n{}",
            parent_id, task
        )
    } else {
        task.to_string()
    };

    let response = runtime.run(&full_prompt, &mut context).await?;

    // Persist assistant response
    let _ = sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) \
         VALUES ($1, $2, 'assistant', $3, now())",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(&response.content)
    .execute(pool)
    .await;

    Ok(response)
}
