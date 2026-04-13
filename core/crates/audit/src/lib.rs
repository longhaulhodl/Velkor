pub mod logger;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Event types — enforced in application layer, not DB constraints (per PRD)
// ---------------------------------------------------------------------------

/// All auditable events in the platform, matching PRD Section 3.3.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEvent {
    // User lifecycle
    UserLogin,
    UserLogout,
    UserCreated,
    UserUpdated,
    UserDeleted,

    // Agent messaging
    AgentMessageSent,
    AgentMessageReceived,

    // Tool usage
    AgentToolCalled,
    AgentToolResult,

    // Skills
    AgentSkillCreated,
    AgentSkillImproved,
    AgentSkillExecuted,

    // Memory
    AgentMemoryStored,
    AgentMemoryUpdated,
    AgentMemoryDeleted,

    // Model calls
    AgentModelCalled,
    AgentModelResponse,

    // Delegation
    AgentDelegated,
    AgentDelegationResult,

    // Documents
    DocumentUploaded,
    DocumentAccessed,
    DocumentDeleted,

    // Scheduling
    ScheduleCreated,
    ScheduleExecuted,
    ScheduleFailed,

    // Retention
    RetentionPolicyApplied,
    RetentionRecordPurged,
    RetentionLegalHoldSet,
    RetentionLegalHoldReleased,

    // Admin
    AdminConfigChanged,
    AdminUserRoleChanged,

    // System
    SystemStartup,
    SystemShutdown,
    SystemError,
}

impl AuditEvent {
    /// Dotted string form used in the database (e.g. "agent.tool.called").
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserLogin => "user.login",
            Self::UserLogout => "user.logout",
            Self::UserCreated => "user.created",
            Self::UserUpdated => "user.updated",
            Self::UserDeleted => "user.deleted",

            Self::AgentMessageSent => "agent.message.sent",
            Self::AgentMessageReceived => "agent.message.received",

            Self::AgentToolCalled => "agent.tool.called",
            Self::AgentToolResult => "agent.tool.result",

            Self::AgentSkillCreated => "agent.skill.created",
            Self::AgentSkillImproved => "agent.skill.improved",
            Self::AgentSkillExecuted => "agent.skill.executed",

            Self::AgentMemoryStored => "agent.memory.stored",
            Self::AgentMemoryUpdated => "agent.memory.updated",
            Self::AgentMemoryDeleted => "agent.memory.deleted",

            Self::AgentModelCalled => "agent.model.called",
            Self::AgentModelResponse => "agent.model.response",

            Self::AgentDelegated => "agent.delegated",
            Self::AgentDelegationResult => "agent.delegation.result",

            Self::DocumentUploaded => "document.uploaded",
            Self::DocumentAccessed => "document.accessed",
            Self::DocumentDeleted => "document.deleted",

            Self::ScheduleCreated => "schedule.created",
            Self::ScheduleExecuted => "schedule.executed",
            Self::ScheduleFailed => "schedule.failed",

            Self::RetentionPolicyApplied => "retention.policy.applied",
            Self::RetentionRecordPurged => "retention.record.purged",
            Self::RetentionLegalHoldSet => "retention.legal_hold.set",
            Self::RetentionLegalHoldReleased => "retention.legal_hold.released",

            Self::AdminConfigChanged => "admin.config.changed",
            Self::AdminUserRoleChanged => "admin.user.role_changed",

            Self::SystemStartup => "system.startup",
            Self::SystemShutdown => "system.shutdown",
            Self::SystemError => "system.error",
        }
    }
}

impl std::fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Audit entry — maps 1:1 to the audit_log table
// ---------------------------------------------------------------------------

/// A single audit log entry ready for insertion.
///
/// Callers build this via [`AuditEntryBuilder`] for ergonomic construction.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub event_type: AuditEvent,
    pub user_id: Option<Uuid>,
    pub agent_id: Option<String>,
    pub conversation_id: Option<Uuid>,
    pub details: JsonValue,
    pub model_used: Option<String>,
    pub tokens_input: Option<i32>,
    pub tokens_output: Option<i32>,
    pub cost_usd: Option<f64>,
    pub ip_address: Option<String>,
    pub request_id: Option<Uuid>,
}

/// A stored audit record with server-generated fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub user_id: Option<Uuid>,
    pub agent_id: Option<String>,
    pub conversation_id: Option<Uuid>,
    pub details: JsonValue,
    pub model_used: Option<String>,
    pub tokens_input: Option<i32>,
    pub tokens_output: Option<i32>,
    pub cost_usd: Option<f64>,
    pub ip_address: Option<String>,
    pub request_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Builder for ergonomic construction
// ---------------------------------------------------------------------------

/// Fluent builder for [`AuditEntry`].
///
/// ```ignore
/// let entry = AuditEntryBuilder::new(AuditEvent::AgentToolCalled)
///     .user_id(user_id)
///     .agent_id("code-assistant")
///     .conversation_id(conv_id)
///     .details(serde_json::json!({"tool": "web_search", "query": "rust async"}))
///     .request_id(req_id)
///     .build();
/// ```
pub struct AuditEntryBuilder {
    entry: AuditEntry,
}

impl AuditEntryBuilder {
    pub fn new(event_type: AuditEvent) -> Self {
        Self {
            entry: AuditEntry {
                event_type,
                user_id: None,
                agent_id: None,
                conversation_id: None,
                details: serde_json::json!({}),
                model_used: None,
                tokens_input: None,
                tokens_output: None,
                cost_usd: None,
                ip_address: None,
                request_id: None,
            },
        }
    }

    pub fn user_id(mut self, id: Uuid) -> Self {
        self.entry.user_id = Some(id);
        self
    }

    pub fn agent_id(mut self, id: impl Into<String>) -> Self {
        self.entry.agent_id = Some(id.into());
        self
    }

    pub fn conversation_id(mut self, id: Uuid) -> Self {
        self.entry.conversation_id = Some(id);
        self
    }

    pub fn details(mut self, details: JsonValue) -> Self {
        self.entry.details = details;
        self
    }

    pub fn model_used(mut self, model: impl Into<String>) -> Self {
        self.entry.model_used = Some(model.into());
        self
    }

    pub fn tokens(mut self, input: i32, output: i32) -> Self {
        self.entry.tokens_input = Some(input);
        self.entry.tokens_output = Some(output);
        self
    }

    pub fn cost_usd(mut self, cost: f64) -> Self {
        self.entry.cost_usd = Some(cost);
        self
    }

    pub fn ip_address(mut self, ip: impl Into<String>) -> Self {
        self.entry.ip_address = Some(ip.into());
        self
    }

    pub fn request_id(mut self, id: Uuid) -> Self {
        self.entry.request_id = Some(id);
        self
    }

    pub fn build(self) -> AuditEntry {
        self.entry
    }
}
