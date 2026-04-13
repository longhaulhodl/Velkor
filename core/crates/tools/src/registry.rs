use crate::{Tool, ToolContext, ToolError, ToolResult};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tracing::debug;

/// Schema representation passed to the LLM so it knows what tools are available.
/// Re-exported from velkor-models for convenience.
pub use velkor_models::ToolSchema;

/// Manages the set of tools available to an agent.
///
/// The registry holds all registered tools and provides:
/// - Schema listing (for sending to the model)
/// - Name-based lookup and execution
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Overwrites any existing tool with the same name.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        debug!(tool = %name, "Registered tool");
        self.tools.insert(name, tool);
    }

    /// Get the JSON schemas for all registered tools (passed to the model).
    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools
            .values()
            .map(|t| ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.input_schema(),
            })
            .collect()
    }

    /// Execute a tool by name with the given input.
    pub async fn execute(
        &self,
        name: &str,
        input: JsonValue,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::ExecutionFailed(format!("unknown tool: {name}")))?;

        tool.execute(input, ctx).await
    }

    /// Check if a tool is registered.
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// List all registered tool names.
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|k| k.as_str()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
