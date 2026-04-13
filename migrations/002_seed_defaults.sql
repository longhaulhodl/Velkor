-- Seed a default workspace for single-workspace Phase 1 usage.
-- Documents need a workspace_id FK, so this must exist before uploads.
INSERT INTO workspaces (id, name, description)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    'Default',
    'Default workspace for document uploads'
) ON CONFLICT (id) DO NOTHING;
