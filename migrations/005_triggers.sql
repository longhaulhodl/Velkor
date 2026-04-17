-- 005_triggers.sql
-- Event-driven triggers (PRD Section 5.2 "Event-Driven Triggers", Section 20 "Webhook Security")
--
-- Two tables:
--   triggers         — definition of a trigger (webhook/file_watch/email), owner, prompt template
--   trigger_events   — queue of fired events awaiting agent processing
--
-- The Pulse engine drives two subsystems:
--   FileWatchSubsystem      — polls file_watch triggers, enqueues events on new/modified files
--   EventProcessorSubsystem — dequeues pending events, runs the configured agent, records result

CREATE TABLE IF NOT EXISTS triggers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('webhook', 'file_watch', 'email')),
    -- kind-specific config:
    --   webhook:    { "secret": "...", "signature_header": "X-Hub-Signature-256", "verify": true, "allowed_ips": [] }
    --   file_watch: { "path": "/abs/path", "events": ["created","modified"], "glob": "*.txt", "poll_every_n_ticks": 1 }
    --   email:      { "mailbox": "...", "filter": "..." }
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    agent_id TEXT NOT NULL DEFAULT 'default',
    -- prompt_template uses {{payload.foo}} / {{file_path}} etc. substitution
    prompt_template TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    -- stats / error tracking
    last_fired_at TIMESTAMPTZ,
    fire_count INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    -- file_watch state: { "<path>": { "mtime": "...", "size": N } } for change detection
    watch_state JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_triggers_user_active
    ON triggers (user_id, is_active);
CREATE INDEX IF NOT EXISTS idx_triggers_kind_active
    ON triggers (kind, is_active)
    WHERE is_active;

CREATE TABLE IF NOT EXISTS trigger_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trigger_id UUID NOT NULL REFERENCES triggers(id) ON DELETE CASCADE,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'done', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    conversation_id UUID,
    source_ip TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);

-- Partial index over pending events — the processor's hot path
CREATE INDEX IF NOT EXISTS idx_trigger_events_pending
    ON trigger_events (created_at)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_trigger_events_trigger
    ON trigger_events (trigger_id, created_at DESC);
