use velkor_memory::MemoryResult;
use velkor_models::{Message, MessageContent, Role};

/// Builds the system prompt injected at the start of each model call.
///
/// Incorporates the agent's persona, core memories (always-present high-importance
/// facts), and recalled memories (query-specific context).
pub struct PromptBuilder;

impl PromptBuilder {
    /// Build the system message from agent config, core memories, and recalled context.
    pub fn build_system_prompt(
        system_instructions: &str,
        core_memories: &[MemoryResult],
        recalled_memories: &[MemoryResult],
    ) -> Message {
        let mut parts = vec![system_instructions.to_string()];

        if !core_memories.is_empty() {
            parts.push("\n## Core Memories (always available)".to_string());
            for mem in core_memories {
                let cat = mem
                    .category
                    .map(|c| c.as_str().to_string())
                    .unwrap_or_else(|| "general".to_string());
                parts.push(format!("- [{}] {}", cat, mem.content));
            }
        }

        if !recalled_memories.is_empty() {
            parts.push("\n## Recalled Memories (relevant to this query)".to_string());
            for mem in recalled_memories {
                parts.push(format!("- {}", mem.content));
            }
        }

        Message {
            role: Role::System,
            content: MessageContent::Text(parts.join("\n")),
        }
    }

    /// Create a user message.
    pub fn user_message(text: &str) -> Message {
        Message {
            role: Role::User,
            content: MessageContent::Text(text.to_string()),
        }
    }

    /// Create an assistant message from model text output.
    pub fn assistant_message(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: MessageContent::Text(text.to_string()),
        }
    }

    /// Create a tool result message to feed back into the model.
    pub fn tool_result_message(tool_use_id: &str, content: &str, is_error: bool) -> Message {
        Message {
            role: Role::Tool,
            content: MessageContent::Blocks(vec![velkor_models::ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: content.to_string(),
                is_error,
            }]),
        }
    }
}
