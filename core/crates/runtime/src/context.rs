use chrono::{DateTime, Utc};
use uuid::Uuid;
use velkor_models::Message;

/// Mutable conversation state threaded through the ReAct loop.
///
/// Holds the message history and identity information for a single
/// conversation turn. The runtime appends assistant/tool messages as
/// the loop progresses.
#[derive(Debug, Clone)]
pub struct ConversationContext {
    pub conversation_id: Uuid,
    pub user_id: Uuid,
    pub agent_id: String,
    pub started_at: DateTime<Utc>,
    /// Full message history for this conversation (grows during the loop).
    pub messages: Vec<Message>,
}

impl ConversationContext {
    pub fn new(conversation_id: Uuid, user_id: Uuid, agent_id: impl Into<String>) -> Self {
        Self {
            conversation_id,
            user_id,
            agent_id: agent_id.into(),
            started_at: Utc::now(),
            messages: Vec::new(),
        }
    }

    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }
}
