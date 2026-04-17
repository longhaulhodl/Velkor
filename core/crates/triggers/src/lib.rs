//! Velkor triggers — event-driven agent execution.
//!
//! Per PRD Section 5.2 ("Event-Driven Triggers") and Section 20 ("Webhook Security").
//!
//! Three trigger kinds are supported:
//! - **webhook**:    external HTTP caller posts JSON → event enqueued → agent fires
//! - **file_watch**: Pulse-driven polling of a directory → new/modified files enqueue events
//! - **email**:      scaffolded only (IMAP polling is not implemented in this pass)
//!
//! Architecture:
//! - `triggers` table holds the definition (kind, config, agent, prompt template).
//! - `trigger_events` is a durable FIFO queue. Anything that fires a trigger
//!   writes a row here; a Pulse subsystem drains it and runs the agent.
//! - Two Pulse subsystems run in-engine:
//!     - `FileWatchSubsystem`      — enqueues events from the filesystem
//!     - `EventProcessorSubsystem` — executes pending events
//! - One HTTP route (outside the crate, in `core/src/routes/webhooks.rs`)
//!   writes webhook events into the queue after HMAC verification.

pub mod crud;
pub mod file_watch;
pub mod processor;
pub mod template;
pub mod webhook;

pub use crud::{
    create_trigger, delete_trigger, get_trigger, list_events, list_triggers, update_trigger,
    TriggerEventInfo, TriggerInfo,
};
pub use file_watch::FileWatchSubsystem;
pub use processor::EventProcessorSubsystem;
pub use webhook::{enqueue_webhook_event, verify_signature, WebhookVerifyError};
