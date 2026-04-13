-- Velkor Agent Platform: Initial Schema
-- Requires PostgreSQL 16+ with pgvector extension

CREATE EXTENSION IF NOT EXISTS "pgcrypto";   -- gen_random_uuid()
CREATE EXTENSION IF NOT EXISTS "vector";      -- pgvector

-- ============================================================
-- Layer 4: Multi-User & Auth
-- ============================================================

CREATE TABLE organizations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    password_hash TEXT NOT NULL,  -- Argon2id
    role TEXT DEFAULT 'member' CHECK (role IN ('admin', 'member', 'viewer')),
    org_id UUID REFERENCES organizations(id),
    settings JSONB DEFAULT '{}',
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT now(),
    last_login_at TIMESTAMPTZ
);

CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    key_hash TEXT NOT NULL,       -- Argon2id hash
    key_prefix TEXT NOT NULL,     -- first 8 chars for identification
    name TEXT,
    permissions JSONB DEFAULT '["*"]',
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- ============================================================
-- Retention & Compliance (referenced by many tables)
-- ============================================================

CREATE TABLE retention_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    applies_to TEXT NOT NULL CHECK (applies_to IN (
        'conversations', 'memories', 'documents', 'skills', 'audit_log'
    )),
    retention_days INTEGER,  -- NULL = keep forever
    action TEXT NOT NULL CHECK (action IN ('delete', 'archive', 'anonymize')),
    requires_review BOOLEAN DEFAULT FALSE,
    is_default BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT now(),
    created_by UUID REFERENCES users(id)
);

CREATE TABLE legal_holds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    scope JSONB NOT NULL,  -- {"users": [...], "conversations": [...], "date_range": {...}}
    created_at TIMESTAMPTZ DEFAULT now(),
    created_by UUID NOT NULL REFERENCES users(id),
    released_at TIMESTAMPTZ,
    released_by UUID REFERENCES users(id),
    is_active BOOLEAN DEFAULT TRUE
);

CREATE TABLE retention_schedule (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    policy_id UUID NOT NULL REFERENCES retention_policies(id),
    record_type TEXT NOT NULL,
    record_id UUID NOT NULL,
    eligible_at TIMESTAMPTZ NOT NULL,
    status TEXT DEFAULT 'pending' CHECK (
        status IN ('pending', 'under_review', 'approved', 'executed', 'held')
    ),
    reviewed_by UUID REFERENCES users(id),
    reviewed_at TIMESTAMPTZ,
    executed_at TIMESTAMPTZ,
    UNIQUE (record_type, record_id)
);

-- ============================================================
-- Conversations & Messages (partitioned by month)
-- ============================================================

CREATE TABLE conversations (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    agent_id TEXT,
    title TEXT,
    summary TEXT,
    summary_embedding vector(1536),
    started_at TIMESTAMPTZ DEFAULT now(),
    ended_at TIMESTAMPTZ,
    message_count INTEGER DEFAULT 0,
    total_tokens INTEGER DEFAULT 0,
    total_cost_usd NUMERIC(10, 6) DEFAULT 0,
    retention_policy_id UUID REFERENCES retention_policies(id),
    is_deleted BOOLEAN DEFAULT FALSE,
    deleted_at TIMESTAMPTZ,
    legal_hold BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (id, started_at)
) PARTITION BY RANGE (started_at);

-- Create initial partitions (one per month, extend as needed)
CREATE TABLE conversations_2026_04 PARTITION OF conversations
    FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');
CREATE TABLE conversations_2026_05 PARTITION OF conversations
    FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');
CREATE TABLE conversations_2026_06 PARTITION OF conversations
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');
CREATE TABLE conversations_default PARTITION OF conversations DEFAULT;

CREATE TABLE messages (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content TEXT NOT NULL,
    tool_calls JSONB,
    tool_results JSONB,
    model_used TEXT,
    tokens_input INTEGER,
    tokens_output INTEGER,
    cost_usd NUMERIC(10, 6),
    created_at TIMESTAMPTZ DEFAULT now(),
    search_vector tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED,
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

CREATE TABLE messages_2026_04 PARTITION OF messages
    FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');
CREATE TABLE messages_2026_05 PARTITION OF messages
    FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');
CREATE TABLE messages_2026_06 PARTITION OF messages
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');
CREATE TABLE messages_default PARTITION OF messages DEFAULT;

CREATE INDEX idx_messages_fts ON messages USING gin (search_vector);
CREATE INDEX idx_messages_conversation ON messages (conversation_id, created_at);

-- ============================================================
-- Memory System
-- ============================================================

CREATE TABLE memories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    org_id UUID REFERENCES organizations(id),
    scope TEXT NOT NULL CHECK (scope IN ('personal', 'shared', 'org')),
    category TEXT CHECK (category IN ('fact', 'preference', 'project', 'procedure', 'relationship')),
    content TEXT NOT NULL,
    embedding vector(1536),
    source_conversation_id UUID,
    confidence REAL DEFAULT 1.0,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),
    expires_at TIMESTAMPTZ,
    retention_policy_id UUID REFERENCES retention_policies(id),
    is_deleted BOOLEAN DEFAULT FALSE,
    deleted_at TIMESTAMPTZ,
    search_vector tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
);

CREATE INDEX idx_memories_embedding ON memories USING hnsw (embedding vector_cosine_ops);
CREATE INDEX idx_memories_fts ON memories USING gin (search_vector);
CREATE INDEX idx_memories_user_scope ON memories (user_id, scope) WHERE NOT is_deleted;

-- ============================================================
-- User Profiles (Tier 6: User Model)
-- ============================================================

CREATE TABLE user_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL UNIQUE REFERENCES users(id),
    profile_data JSONB NOT NULL DEFAULT '{}',
    profile_embedding vector(1536),
    last_updated TIMESTAMPTZ DEFAULT now(),
    update_count INTEGER DEFAULT 0
);

-- ============================================================
-- Skills (Tier 5: Procedural Memory)
-- ============================================================

CREATE TABLE skills (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    content TEXT NOT NULL,
    content_embedding vector(1536),
    category TEXT,
    author TEXT NOT NULL,  -- 'system', 'user:{id}', or 'agent:{id}'
    source_conversation_id UUID,
    usage_count INTEGER DEFAULT 0,
    success_rate REAL DEFAULT 1.0,
    last_used_at TIMESTAMPTZ,
    last_improved_at TIMESTAMPTZ,
    version INTEGER DEFAULT 1,
    created_at TIMESTAMPTZ DEFAULT now(),
    retention_policy_id UUID REFERENCES retention_policies(id),
    is_active BOOLEAN DEFAULT TRUE,
    search_vector tsvector GENERATED ALWAYS AS (
        to_tsvector('english', coalesce(name, '') || ' ' || coalesce(description, '') || ' ' || content)
    ) STORED
);

CREATE INDEX idx_skills_embedding ON skills USING hnsw (content_embedding vector_cosine_ops);
CREATE INDEX idx_skills_fts ON skills USING gin (search_vector);

-- ============================================================
-- Document Workspace
-- ============================================================

CREATE TABLE workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id UUID REFERENCES organizations(id),
    name TEXT NOT NULL,
    description TEXT,
    connector_type TEXT DEFAULT 'local' CHECK (
        connector_type IN ('local', 'google_drive', 'sharepoint', 's3', 'dropbox')
    ),
    connector_config JSONB,
    retention_policy_id UUID REFERENCES retention_policies(id),
    created_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    user_id UUID NOT NULL REFERENCES users(id),
    filename TEXT NOT NULL,
    mime_type TEXT,
    file_size BIGINT,
    storage_key TEXT NOT NULL,
    content_text TEXT,
    content_embedding vector(1536),
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),
    retention_policy_id UUID REFERENCES retention_policies(id),
    is_deleted BOOLEAN DEFAULT FALSE,
    deleted_at TIMESTAMPTZ,
    legal_hold BOOLEAN DEFAULT FALSE,
    search_vector tsvector GENERATED ALWAYS AS (
        to_tsvector('english', coalesce(filename, '') || ' ' || coalesce(content_text, ''))
    ) STORED
);

CREATE INDEX idx_documents_fts ON documents USING gin (search_vector);
CREATE INDEX idx_documents_embedding ON documents USING hnsw (content_embedding vector_cosine_ops);
CREATE INDEX idx_documents_workspace ON documents (workspace_id) WHERE NOT is_deleted;

-- ============================================================
-- Scheduling
-- ============================================================

CREATE TABLE schedules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    agent_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    cron_expression TEXT NOT NULL,
    natural_language TEXT,
    task_prompt TEXT NOT NULL,
    delivery_channel TEXT DEFAULT 'web',
    delivery_target TEXT,
    is_active BOOLEAN DEFAULT TRUE,
    last_run_at TIMESTAMPTZ,
    next_run_at TIMESTAMPTZ,
    run_count INTEGER DEFAULT 0,
    error_count INTEGER DEFAULT 0,
    last_error TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    retention_policy_id UUID REFERENCES retention_policies(id)
);

CREATE TABLE schedule_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schedule_id UUID NOT NULL REFERENCES schedules(id),
    started_at TIMESTAMPTZ DEFAULT now(),
    completed_at TIMESTAMPTZ,
    status TEXT CHECK (status IN ('running', 'completed', 'failed', 'cancelled')),
    result_summary TEXT,
    conversation_id UUID,
    tokens_used INTEGER,
    cost_usd NUMERIC(10, 6),
    error TEXT
);

-- ============================================================
-- Audit Log (append-only, partitioned by month)
-- ============================================================

CREATE TABLE audit_log (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ DEFAULT now(),
    event_type TEXT NOT NULL,
    user_id UUID,
    agent_id TEXT,
    conversation_id UUID,
    details JSONB NOT NULL,
    model_used TEXT,
    tokens_input INTEGER,
    tokens_output INTEGER,
    cost_usd NUMERIC(10, 6),
    ip_address INET,
    request_id UUID,
    PRIMARY KEY (id, timestamp)
) PARTITION BY RANGE (timestamp);

CREATE TABLE audit_log_2026_04 PARTITION OF audit_log
    FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');
CREATE TABLE audit_log_2026_05 PARTITION OF audit_log
    FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');
CREATE TABLE audit_log_2026_06 PARTITION OF audit_log
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');
CREATE TABLE audit_log_default PARTITION OF audit_log DEFAULT;

CREATE INDEX idx_audit_timestamp ON audit_log (timestamp);
CREATE INDEX idx_audit_user ON audit_log (user_id, timestamp);
CREATE INDEX idx_audit_type ON audit_log (event_type, timestamp);
CREATE INDEX idx_audit_conversation ON audit_log (conversation_id) WHERE conversation_id IS NOT NULL;
