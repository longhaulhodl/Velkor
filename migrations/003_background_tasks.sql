-- Background tasks: long-running agent work spawned on demand.
-- Users can kick off a task and continue chatting while it runs.

CREATE TABLE IF NOT EXISTS background_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    agent_id TEXT NOT NULL DEFAULT 'default',
    title TEXT NOT NULL,
    task_prompt TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    result_summary TEXT,
    conversation_id UUID,  -- conversation created for this task's execution
    source_conversation_id UUID,  -- conversation the task was spawned from (for context)
    tokens_used INTEGER DEFAULT 0,
    cost_usd NUMERIC(10, 6),
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_background_tasks_user_status ON background_tasks (user_id, status);
CREATE INDEX IF NOT EXISTS idx_background_tasks_created ON background_tasks (created_at DESC);
