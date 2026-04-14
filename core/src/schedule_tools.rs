use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use tracing::debug;
use velkor_tools::{Tool, ToolContext, ToolError, ToolResult};

// ---------------------------------------------------------------------------
// schedule_list
// ---------------------------------------------------------------------------

/// Lists the user's scheduled tasks (cron jobs).
pub struct ScheduleListTool {
    pool: PgPool,
}

impl ScheduleListTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for ScheduleListTool {
    fn name(&self) -> &str {
        "schedule_list"
    }

    fn description(&self) -> &str {
        "List the user's scheduled tasks (cron jobs). Shows name, cron expression, next run time, run count, and whether active."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let schedules = velkor_scheduler::list_schedules(&self.pool, Some(ctx.user_id))
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("failed to list schedules: {e}")))?;

        if schedules.is_empty() {
            return Ok(ToolResult::success("No scheduled tasks found."));
        }

        let mut output = format!("Found {} schedule(s):\n\n", schedules.len());
        for s in &schedules {
            output.push_str(&format!(
                "- **{}** (id: {})\n  Cron: `{}`{}\n  Agent: {} | Active: {} | Runs: {} | Errors: {}\n  Next run: {}\n  Prompt: {}\n\n",
                s.name,
                s.id,
                s.cron_expression,
                s.natural_language.as_deref().map(|nl| format!(" ({})", nl)).unwrap_or_default(),
                s.agent_id,
                if s.is_active { "yes" } else { "no" },
                s.run_count,
                s.error_count,
                s.next_run_at.map(|t| t.to_string()).unwrap_or_else(|| "not scheduled".into()),
                if s.task_prompt.len() > 100 {
                    format!("{}...", &s.task_prompt[..100])
                } else {
                    s.task_prompt.clone()
                },
            ));
        }
        Ok(ToolResult::success(output))
    }
}

// ---------------------------------------------------------------------------
// schedule_create
// ---------------------------------------------------------------------------

/// Creates a new scheduled task (cron job).
pub struct ScheduleCreateTool {
    pool: PgPool,
}

impl ScheduleCreateTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for ScheduleCreateTool {
    fn name(&self) -> &str {
        "schedule_create"
    }

    fn description(&self) -> &str {
        "Create a new scheduled task (cron job). The task will run automatically on the specified cron schedule. Use standard 5-field cron expressions (minute hour day-of-month month day-of-week)."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Short name for the schedule"
                },
                "cron_expression": {
                    "type": "string",
                    "description": "Cron expression (5-field: min hour dom month dow). Examples: '0 7 * * 1-5' (weekdays 7am), '*/15 * * * *' (every 15 min), '0 0 1 * *' (monthly)"
                },
                "task_prompt": {
                    "type": "string",
                    "description": "The prompt/instructions the agent will execute on each run"
                },
                "description": {
                    "type": "string",
                    "description": "Optional description of what this schedule does"
                },
                "natural_language": {
                    "type": "string",
                    "description": "Optional human-readable description of the timing (e.g. 'every weekday at 7am')"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to run the task (default: 'default')"
                }
            },
            "required": ["name", "cron_expression", "task_prompt"]
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let name = input["name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("missing 'name'".into()))?;
        let cron_expression = input["cron_expression"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("missing 'cron_expression'".into()))?;
        let task_prompt = input["task_prompt"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("missing 'task_prompt'".into()))?;
        let description = input["description"].as_str();
        let natural_language = input["natural_language"].as_str();
        let agent_id = input["agent_id"].as_str().unwrap_or("default");

        debug!(name, cron_expression, "Creating schedule via tool");

        let schedule = velkor_scheduler::create_schedule(
            &self.pool,
            ctx.user_id,
            agent_id,
            name,
            description,
            cron_expression,
            natural_language,
            task_prompt,
            None, // delivery_channel
            None, // delivery_target
        )
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("failed to create schedule: {e}")))?;

        Ok(ToolResult::success(format!(
            "Schedule '{}' created successfully (id: {}). Next run: {}",
            schedule.name,
            schedule.id,
            schedule.next_run_at.map(|t| t.to_string()).unwrap_or_else(|| "calculating...".into()),
        )))
    }
}

// ---------------------------------------------------------------------------
// schedule_update
// ---------------------------------------------------------------------------

/// Updates an existing scheduled task.
pub struct ScheduleUpdateTool {
    pool: PgPool,
}

impl ScheduleUpdateTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for ScheduleUpdateTool {
    fn name(&self) -> &str {
        "schedule_update"
    }

    fn description(&self) -> &str {
        "Update an existing scheduled task. Can change name, cron expression, task prompt, or pause/resume. Use schedule_list first to find the schedule ID."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "schedule_id": {
                    "type": "string",
                    "description": "The UUID of the schedule to update"
                },
                "name": {
                    "type": "string",
                    "description": "New name (optional)"
                },
                "cron_expression": {
                    "type": "string",
                    "description": "New cron expression (optional)"
                },
                "task_prompt": {
                    "type": "string",
                    "description": "New task prompt (optional)"
                },
                "is_active": {
                    "type": "boolean",
                    "description": "Set to false to pause, true to resume"
                },
                "description": {
                    "type": "string",
                    "description": "New description (optional)"
                },
                "natural_language": {
                    "type": "string",
                    "description": "New natural language timing description (optional)"
                }
            },
            "required": ["schedule_id"]
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let schedule_id: uuid::Uuid = input["schedule_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("missing 'schedule_id'".into()))?
            .parse()
            .map_err(|_| ToolError::InvalidInput("invalid schedule_id UUID".into()))?;

        // Verify ownership
        let existing = velkor_scheduler::get_schedule(&self.pool, schedule_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("failed to get schedule: {e}")))?
            .ok_or_else(|| ToolError::InvalidInput("schedule not found".into()))?;

        if existing.user_id != ctx.user_id {
            return Err(ToolError::PermissionDenied("not your schedule".into()));
        }

        let result = velkor_scheduler::update_schedule(
            &self.pool,
            schedule_id,
            input["name"].as_str(),
            input["description"].as_str(),
            input["cron_expression"].as_str(),
            input["natural_language"].as_str(),
            input["task_prompt"].as_str(),
            None, // delivery_channel
            None, // delivery_target
            input["is_active"].as_bool(),
        )
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("failed to update schedule: {e}")))?;

        let updated = result.ok_or_else(|| ToolError::InvalidInput("schedule not found".into()))?;

        Ok(ToolResult::success(format!(
            "Schedule '{}' updated. Active: {}. Next run: {}",
            updated.name,
            if updated.is_active { "yes" } else { "paused" },
            updated.next_run_at.map(|t| t.to_string()).unwrap_or_else(|| "N/A".into()),
        )))
    }
}

// ---------------------------------------------------------------------------
// schedule_delete
// ---------------------------------------------------------------------------

/// Deletes a scheduled task and its run history.
pub struct ScheduleDeleteTool {
    pool: PgPool,
}

impl ScheduleDeleteTool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for ScheduleDeleteTool {
    fn name(&self) -> &str {
        "schedule_delete"
    }

    fn description(&self) -> &str {
        "Delete a scheduled task and all its run history. This is permanent. Use schedule_list first to find the schedule ID."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "schedule_id": {
                    "type": "string",
                    "description": "The UUID of the schedule to delete"
                }
            },
            "required": ["schedule_id"]
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let schedule_id: uuid::Uuid = input["schedule_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput("missing 'schedule_id'".into()))?
            .parse()
            .map_err(|_| ToolError::InvalidInput("invalid schedule_id UUID".into()))?;

        // Verify ownership
        let existing = velkor_scheduler::get_schedule(&self.pool, schedule_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("failed to get schedule: {e}")))?
            .ok_or_else(|| ToolError::InvalidInput("schedule not found".into()))?;

        if existing.user_id != ctx.user_id {
            return Err(ToolError::PermissionDenied("not your schedule".into()));
        }

        let deleted = velkor_scheduler::delete_schedule(&self.pool, schedule_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("failed to delete schedule: {e}")))?;

        if deleted {
            Ok(ToolResult::success(format!(
                "Schedule '{}' deleted along with all run history.",
                existing.name,
            )))
        } else {
            Ok(ToolResult::error("Schedule not found or already deleted."))
        }
    }
}
