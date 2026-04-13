-- Seed a default workspace for single-workspace Phase 1 usage.
-- Documents need a workspace_id FK, so this must exist before uploads.
INSERT INTO workspaces (id, name, description)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    'Default',
    'Default workspace for document uploads'
) ON CONFLICT (id) DO NOTHING;

-- The conversations table is partitioned with PK (id, started_at).
-- The chat upsert needs ON CONFLICT on id alone, so add a unique index.
CREATE UNIQUE INDEX IF NOT EXISTS idx_conversations_id_unique
    ON conversations (id);
