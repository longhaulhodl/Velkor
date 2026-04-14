//! Skills tools — skill_list, skill_view, skill_manage.
//!
//! These follow the Hermes pattern:
//! - skill_list: returns all skill names + descriptions (already in prompt, but
//!   useful for explicit listing with metadata)
//! - skill_view: load full skill content (progressive disclosure tier 2)
//! - skill_manage: create, patch, or deactivate learned skills

use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tokio::sync::RwLock;
use velkor_skills::store::SkillStore;

/// Shared skill store handle used by all skill tools.
pub type SkillStoreHandle = Arc<RwLock<SkillStore>>;

// ---------------------------------------------------------------------------
// skill_list
// ---------------------------------------------------------------------------

pub struct SkillListTool {
    store: SkillStoreHandle,
}

impl SkillListTool {
    pub fn new(store: SkillStoreHandle) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for SkillListTool {
    fn name(&self) -> &str {
        "skill_list"
    }

    fn description(&self) -> &str {
        "List all available skills with their names, descriptions, and metadata. \
         Use this to discover what skills are available before loading one with skill_view."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _input: JsonValue, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let store = self.store.read().await;
        let summaries = store.all_skill_summaries().await;

        if summaries.is_empty() {
            return Ok(ToolResult::success("No skills available."));
        }

        let mut lines = Vec::new();
        for (name, desc, source) in &summaries {
            lines.push(format!("- {name} [{source}]: {desc}"));
        }

        Ok(ToolResult::success(lines.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// skill_view
// ---------------------------------------------------------------------------

pub struct SkillViewTool {
    store: SkillStoreHandle,
}

impl SkillViewTool {
    pub fn new(store: SkillStoreHandle) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for SkillViewTool {
    fn name(&self) -> &str {
        "skill_view"
    }

    fn description(&self) -> &str {
        "Load the full content of a skill by name. Use this to read a skill's instructions \
         before following them. Returns the complete SKILL.md body."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The skill name to load"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, input: JsonValue, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let name = input
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'name' field".into()))?;

        let store = self.store.read().await;

        // Check installable first
        if let Some(skill) = store.get_installable(name) {
            let mut output = format!(
                "# {} (installed)\n\n**Description:** {}\n",
                skill.frontmatter.name, skill.frontmatter.description
            );
            if let Some(ref v) = skill.frontmatter.version {
                output.push_str(&format!("**Version:** {v}\n"));
            }
            if let Some(ref a) = skill.frontmatter.author {
                output.push_str(&format!("**Author:** {a}\n"));
            }
            output.push_str(&format!("\n---\n\n{}", skill.body));
            return Ok(ToolResult::success(output));
        }

        // Check learned
        if let Ok(Some(skill)) = store.get_learned_by_name(name).await {
            let desc = skill.description.as_deref().unwrap_or("(no description)");
            let output = format!(
                "# {} (learned, v{})\n\n**Description:** {}\n**Usage count:** {}\n**Success rate:** {:.0}%\n\n---\n\n{}",
                skill.name, skill.version, desc,
                skill.usage_count, skill.success_rate * 100.0,
                skill.content
            );
            return Ok(ToolResult::success(output));
        }

        Ok(ToolResult::error(format!("Skill '{name}' not found.")))
    }
}

// ---------------------------------------------------------------------------
// skill_manage
// ---------------------------------------------------------------------------

pub struct SkillManageTool {
    store: SkillStoreHandle,
}

impl SkillManageTool {
    pub fn new(store: SkillStoreHandle) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for SkillManageTool {
    fn name(&self) -> &str {
        "skill_manage"
    }

    fn description(&self) -> &str {
        "Create, patch, or deactivate a learned skill. Use 'create' to save a new reusable \
         approach after completing a complex task. Use 'patch' to update an existing skill \
         with improvements or corrections. Use 'deactivate' to disable a skill that is no \
         longer useful."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "patch", "deactivate"],
                    "description": "The action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Skill name (required for create/patch/deactivate)"
                },
                "description": {
                    "type": "string",
                    "description": "Brief description of what the skill does and when to use it (for create/patch)"
                },
                "content": {
                    "type": "string",
                    "description": "The skill's full instructions/procedure (for create/patch)"
                },
                "category": {
                    "type": "string",
                    "description": "Optional category for organization (for create)"
                }
            },
            "required": ["action", "name"]
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'action' field".into()))?;

        let name = input
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'name' field".into()))?;

        let store = self.store.write().await;

        match action {
            "create" => {
                let content = input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidInput("missing 'content' for create action".into())
                    })?;

                let description = input.get("description").and_then(|v| v.as_str());
                let category = input.get("category").and_then(|v| v.as_str());

                // Check if skill already exists
                if store.get_installable(name).is_some() {
                    return Ok(ToolResult::error(format!(
                        "An installable skill named '{name}' already exists. Use a different name."
                    )));
                }
                if let Ok(Some(_)) = store.get_learned_by_name(name).await {
                    return Ok(ToolResult::error(format!(
                        "A learned skill named '{name}' already exists. Use 'patch' to update it."
                    )));
                }

                match store
                    .create_learned(
                        name,
                        description,
                        content,
                        category,
                        "agent",
                        Some(ctx.conversation_id),
                    )
                    .await
                {
                    Ok(skill) => Ok(ToolResult::success(format!(
                        "Skill '{}' created (id: {}).",
                        skill.name, skill.id
                    ))),
                    Err(e) => Ok(ToolResult::error(format!("Failed to create skill: {e}"))),
                }
            }

            "patch" => {
                let content = input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidInput("missing 'content' for patch action".into())
                    })?;

                let description = input.get("description").and_then(|v| v.as_str());

                // Find the existing learned skill
                let existing = store
                    .get_learned_by_name(name)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
                    .ok_or_else(|| {
                        ToolError::ExecutionFailed(format!(
                            "No learned skill named '{name}' found to patch."
                        ))
                    })?;

                match store.patch_learned(existing.id, content, description).await {
                    Ok(()) => Ok(ToolResult::success(format!(
                        "Skill '{}' patched (now v{}).",
                        name,
                        existing.version + 1
                    ))),
                    Err(e) => Ok(ToolResult::error(format!("Failed to patch skill: {e}"))),
                }
            }

            "deactivate" => {
                let existing = store
                    .get_learned_by_name(name)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
                    .ok_or_else(|| {
                        ToolError::ExecutionFailed(format!(
                            "No learned skill named '{name}' found to deactivate."
                        ))
                    })?;

                match store.deactivate_learned(existing.id).await {
                    Ok(()) => Ok(ToolResult::success(format!(
                        "Skill '{}' deactivated.",
                        name
                    ))),
                    Err(e) => Ok(ToolResult::error(format!(
                        "Failed to deactivate skill: {e}"
                    ))),
                }
            }

            other => Err(ToolError::InvalidInput(format!(
                "unknown action '{other}', expected 'create', 'patch', or 'deactivate'"
            ))),
        }
    }
}
