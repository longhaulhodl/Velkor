-- Add importance scoring to memories for quality-gated storage.
-- Scale: 1 (trivial) to 10 (critical). Default 5 (moderate).
-- Memories below the configured threshold are rejected at storage time.

ALTER TABLE memories ADD COLUMN IF NOT EXISTS importance SMALLINT DEFAULT 5
    CHECK (importance >= 1 AND importance <= 10);

-- Index for core memory retrieval (high-importance memories always in prompt)
CREATE INDEX IF NOT EXISTS idx_memories_core
    ON memories (user_id, importance DESC, created_at DESC)
    WHERE NOT is_deleted AND importance >= 8;
