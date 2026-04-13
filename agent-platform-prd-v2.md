# Project Plan: Provider-Agnostic Agent Platform

## Document Purpose

This is the master project plan and product requirements document (PRD) for building a provider-agnostic, multi-agent orchestration platform. This document is intended to be used directly with Claude Code as the primary implementation reference. Every section contains enough detail to begin implementation without ambiguity.

**Design principle: Built to last.** Every technology choice in this document is selected for longevity, production reliability, and scale. No "good enough for MVP" compromises on foundational infrastructure. The database, the language, the memory architecture — these are decisions that compound over years. Get them right once.

---

## 1. Vision & Positioning

### What This Is

A self-hosted, provider-agnostic agent platform with multi-agent orchestration, self-improving skills, persistent memory, a polished web UI, a document workspace, scheduling/heartbeat automation, multi-user support, and compliance-grade data retention — built for power users and teams who are not necessarily developers.

### What Makes It Different

| Competitor | Gap This Product Fills |
|---|---|
| OpenClaw | Messaging-first, no self-learning loop, no document workspace, no retention/compliance, no multi-user, sprawling complexity, Node.js-only |
| Hermes Agent | CLI-first, narrow UI, Nous-centric DNA, no document workspace, no retention/compliance, no multi-user, Python single-threaded limitations |
| ChatGPT / Claude.ai | Cloud-only, no self-hosting, no multi-agent, no scheduling, no retention control, no provider choice |

### Core Value Proposition

> One platform. Any model. Multi-agent orchestration that learns and improves. Persistent memory. Document workspace. Scheduling that works while you sleep. Built for teams. Compliance-ready from day one.

---

## 2. Architecture Overview

### System Layers

```
┌─────────────────────────────────────────────────────────────────┐
│                        LAYER 6: WEB UI                          │
│  Dashboard · Agent Chat · Memory Browser · Skill Library        │
│  Document Workspace · Scheduling Manager · Admin/Settings       │
│  Retention & Audit Dashboard                                    │
├─────────────────────────────────────────────────────────────────┤
│                    LAYER 5: CHANNELS & API                      │
│  Web UI (primary) · REST API · WebSocket · Telegram · Discord   │
│  Slack · Email · Webhooks · MCP Server Support                  │
├─────────────────────────────────────────────────────────────────┤
│                LAYER 4: MULTI-USER & AUTH                       │
│  User accounts · Roles (admin/member/viewer) · Per-user memory  │
│  Shared org memory · Usage tracking · Cost attribution          │
│  Session management · API key management                        │
├─────────────────────────────────────────────────────────────────┤
│              LAYER 3: ORCHESTRATION & SCHEDULING                │
│  Supervisor agent pattern · Agent-to-agent delegation           │
│  Parallel agent execution · Task queue · Cron scheduler         │
│  Heartbeat monitor · Event-driven triggers                      │
├─────────────────────────────────────────────────────────────────┤
│                LAYER 2: AGENT RUNTIME                           │
│  ReAct reasoning loop · Tool calling · Streaming responses      │
│  Self-improving skill loop · Skill discovery & execution        │
│  Provider-agnostic model routing · Context management           │
├─────────────────────────────────────────────────────────────────┤
│              LAYER 1: FOUNDATION SERVICES                       │
│  Memory System · Document Store · Audit Logger                  │
│  Retention Engine · Config Management · Plugin System            │
│  Database · Vector Store · Cache · Object Storage               │
└─────────────────────────────────────────────────────────────────┘
```

### Technology Stack — Built to Last

Every choice below is made for 10-year longevity, production reliability at scale, hiring pool depth, and ecosystem maturity.

#### Language: Rust (core runtime) + TypeScript (web UI + API layer)

**Why Rust for the core agent runtime, memory engine, scheduling, and orchestration:**

- Memory safety without garbage collection — no GC pauses during real-time agent streaming
- Fearless concurrency via the ownership model — multi-agent parallel execution without race conditions
- Performance: benchmarks show Rust LLM gateways handle 5,000+ RPS with sub-50ms p95 latency vs. Python's 3,400x performance gap under load
- Compiles to a single binary — trivial deployment, Docker images under 50MB
- The U.S. federal government has mandated migration from C/C++ to memory-safe languages; Rust is the primary target
- Rust's compiler catches bugs that would be runtime crashes in Python or Go — AI coding assistants produce valid Rust that the compiler verifies
- Growing AI/agent ecosystem: Rust-based LLM gateways (Bifrost, VidaiServer), embedding libraries, and inference runtimes are production-proven
- "Rust reduces technical debt through strong compiler checks and memory safety guarantees, minimizing costly production bugs" — this matters when you're a solo founder who can't afford to debug memory leaks at 3am

**Why TypeScript for the API layer and web UI:**

- React + TypeScript is the dominant frontend stack with the deepest talent pool
- FastAPI-equivalent frameworks exist in the Node/Deno ecosystem (Hono, Elysia) but the API layer is thin — it's a gateway to the Rust core
- Full-stack TypeScript means the API routes, WebSocket handlers, and React frontend share types
- AI agent startups are increasingly choosing TypeScript for their product layers
- Next.js or similar for SSR where needed

**The polyglot architecture pattern emerging in 2026:**

| Concern | Language | Rationale |
|---|---|---|
| Agent runtime, memory engine, orchestrator, scheduler | Rust | Performance, safety, concurrency, single-binary deployment |
| API gateway, WebSocket server, channel integrations | TypeScript (Deno/Bun or Node) | Developer velocity, shared types with frontend, ecosystem |
| Web UI | React + TypeScript | Dominant ecosystem, component libraries, talent pool |
| Skills/plugins (user-authored) | Python + TypeScript | Users bring their own; platform provides sandbox |

**Communication between Rust core and TypeScript API layer:**

The Rust core exposes a gRPC or native FFI interface. The TypeScript API layer calls into it. This is a proven pattern (Prisma's Rust query engine + TypeScript API, Turborepo's Rust core + TypeScript CLI). Alternatively, the Rust core can expose an HTTP API that the TypeScript layer proxies — simpler to start, easy to optimize later.

#### Database: PostgreSQL + pgvector (primary) with Redis (cache/session)

**Why PostgreSQL as the single source of truth:**

- 30+ years of production reliability. The most battle-tested relational database in existence
- pgvector extension provides native vector similarity search (HNSW indexing) — no separate vector database needed for the first several million embeddings
- Full-text search via tsvector — no separate search engine needed initially
- ACID transactions across all data types: conversations, memories, documents, audit logs, vectors — in one transaction
- Horizontal read scaling via replicas; write scaling via partitioning and Citus extension if needed
- Every cloud provider offers managed PostgreSQL (RDS, Cloud SQL, Supabase, Neon, Render)
- The largest hiring pool of any database technology. Every backend developer knows SQL
- Alembic (Python) or refinery (Rust) for schema migrations — the schema evolves safely over years
- OpenAI scaled PostgreSQL to 800 million users using read replicas

**Why NOT SurrealDB (despite impressive multi-model capabilities):**

SurrealDB is architecturally interesting (unified relational + graph + vector + document in one engine, used by Samsung/Verizon, $44M raised, v3.0 just launched). However, for a product that needs to last:
- SurrealDB is ~3 years old. PostgreSQL is ~30 years old.
- Talent pool for SurrealQL is tiny compared to SQL
- Operational tooling, monitoring, backup/restore, and managed hosting options are immature compared to Postgres
- If SurrealDB proves itself over the next 2-3 years, a migration path exists. The reverse is also true — starting on Postgres is the safe bet with an upgrade path
- PostgreSQL + pgvector + graph queries via recursive CTEs covers 95% of agent memory needs

**Why Redis for caching and session state:**

- In-memory, sub-millisecond latency for hot data: active conversation context, agent session state, rate limiting, WebSocket connection tracking
- Redis Streams for real-time event distribution (agent activity feeds, tool call notifications)
- Mature, battle-tested, available as managed service everywhere
- Pub/sub for multi-instance coordination when scaling horizontally

**Database architecture summary:**

```
┌──────────────────────────────────────────────────┐
│                   PostgreSQL                      │
│                                                    │
│  Conversations & Messages (partitioned by date)   │
│  Memories (+ pgvector HNSW index for embeddings)  │
│  Documents (metadata; files in object storage)    │
│  Skills (content + embeddings)                    │
│  Users, Orgs, API Keys                            │
│  Schedules & Schedule Runs                        │
│  Retention Policies & Legal Holds                 │
│  Audit Log (append-only, partitioned by month)    │
│  Full-Text Search (tsvector indexes)              │
│                                                    │
├──────────────────────────────────────────────────┤
│                     Redis                         │
│                                                    │
│  Active conversation context (hot cache)          │
│  Agent session state                              │
│  Rate limiting counters                           │
│  WebSocket connection registry                    │
│  Pub/sub for real-time events                     │
│  Task queue (Redis Streams or BullMQ)             │
│                                                    │
├──────────────────────────────────────────────────┤
│              Object Storage (S3/MinIO)            │
│                                                    │
│  Uploaded documents (PDFs, DOCX, images)          │
│  Generated files and exports                      │
│  Audit log archives                               │
│  Backup snapshots                                 │
└──────────────────────────────────────────────────┘
```

#### Frontend: React + TypeScript + Tailwind CSS

- React 19+ with Server Components where beneficial
- Tailwind CSS for rapid, consistent styling — your chosen design direction applied as a design system
- Zustand for state management (lightweight, no boilerplate)
- TanStack Query for server state / API caching
- WebSocket for real-time streaming chat and agent activity

#### Model Routing: LiteLLM (or Rust equivalent)

- LiteLLM provides a unified OpenAI-compatible interface to 100+ providers
- If building the model router in Rust, use the OpenAI API format as the standard and implement provider adapters
- OpenRouter as a meta-provider option (200+ models, single API key)
- Cost tracking per request, per model, per user

#### Containerization: Docker + Docker Compose (self-hosted) / Kubernetes (scaled)

- Single `docker compose up` for self-hosted deployment
- Multi-service: rust-core, api-gateway, frontend, postgres, redis, minio
- Kubernetes Helm chart for enterprise/hosted deployment
- Each service independently scalable

#### Full Stack Summary

| Layer | Technology | Why It Lasts |
|---|---|---|
| Core runtime | Rust | Memory-safe, fast, single binary, compiler-verified correctness |
| API gateway | TypeScript (Hono/Express) | Ecosystem depth, shared types with UI, developer velocity |
| Frontend | React + TypeScript + Tailwind | Dominant ecosystem, deepest talent pool, component libraries |
| Primary database | PostgreSQL 16+ | 30 years proven, ACID, pgvector, managed everywhere |
| Vector search | pgvector (built into Postgres) | No extra infrastructure, scales to millions of embeddings |
| Full-text search | PostgreSQL tsvector | Built-in, no Elasticsearch dependency |
| Cache / sessions | Redis 7+ | Sub-ms latency, Streams, Pub/Sub, battle-tested |
| Object storage | S3-compatible (MinIO self-hosted) | Industry standard, infinite scale for documents |
| Task queue | Redis Streams or Rust-native (Tokio tasks) | No separate queue infrastructure for MVP |
| Migrations | refinery (Rust) or dbmate | Schema evolution tracked in version control |
| Auth | JWT + Argon2 (not bcrypt) | Argon2 is the modern winner of the Password Hashing Competition |
| Containerization | Docker Compose / Kubernetes | Universal deployment standard |

---

## 3. Foundation Services (Layer 1)

### 3.1 Memory System

Memory is the most critical differentiator. The system uses a multi-tier memory architecture, all backed by PostgreSQL.

#### Memory Tiers

**Tier 1: Conversation Context (Short-Term)**
- Current conversation messages and tool results
- Smart context compression when approaching token limits
- Sliding window with summarization of older messages
- Hot cache in Redis for active conversations
- Per-conversation, ephemeral after conversation ends (summarized into long-term)

**Tier 2: Working Memory (Session)**
- Active project context, current task state
- Scratchpad for multi-step reasoning
- Stored in Redis for speed, persisted to Postgres on session end

**Tier 3: Long-Term Factual Memory**
- Facts, preferences, project details learned from conversations
- Structured as records with metadata (source conversation, timestamp, confidence)
- Searchable via FTS (tsvector) and vector similarity (pgvector)
- Per-user and per-organization scopes

```sql
CREATE TABLE memories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    org_id UUID REFERENCES organizations(id),
    scope TEXT NOT NULL CHECK (scope IN ('personal', 'shared', 'org')),
    category TEXT CHECK (category IN ('fact', 'preference', 'project', 'procedure', 'relationship')),
    content TEXT NOT NULL,
    embedding vector(1536), -- pgvector: dimensionality matches embedding model
    source_conversation_id UUID REFERENCES conversations(id),
    confidence REAL DEFAULT 1.0,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),
    expires_at TIMESTAMPTZ, -- retention policy
    retention_policy_id UUID REFERENCES retention_policies(id),
    is_deleted BOOLEAN DEFAULT FALSE,
    deleted_at TIMESTAMPTZ,
    search_vector tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
);

CREATE INDEX idx_memories_embedding ON memories USING hnsw (embedding vector_cosine_ops);
CREATE INDEX idx_memories_fts ON memories USING gin (search_vector);
CREATE INDEX idx_memories_user_scope ON memories (user_id, scope) WHERE NOT is_deleted;
```

**Tier 4: Episodic Memory (Conversation History)**
- Full searchable history of past conversations
- FTS across all messages + vector similarity for semantic search
- LLM-powered summarization stored per conversation

```sql
CREATE TABLE conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    agent_id TEXT,
    title TEXT,
    summary TEXT, -- LLM-generated summary
    summary_embedding vector(1536),
    started_at TIMESTAMPTZ DEFAULT now(),
    ended_at TIMESTAMPTZ,
    message_count INTEGER DEFAULT 0,
    total_tokens INTEGER DEFAULT 0,
    total_cost_usd NUMERIC(10, 6) DEFAULT 0,
    retention_policy_id UUID REFERENCES retention_policies(id),
    is_deleted BOOLEAN DEFAULT FALSE,
    deleted_at TIMESTAMPTZ,
    legal_hold BOOLEAN DEFAULT FALSE
) PARTITION BY RANGE (started_at);

-- Create partitions by month for efficient retention and archival
-- Example: CREATE TABLE conversations_2026_04 PARTITION OF conversations
--          FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');

CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content TEXT NOT NULL,
    tool_calls JSONB, -- structured tool call data
    tool_results JSONB,
    model_used TEXT,
    tokens_input INTEGER,
    tokens_output INTEGER,
    cost_usd NUMERIC(10, 6),
    created_at TIMESTAMPTZ DEFAULT now(),
    search_vector tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
) PARTITION BY RANGE (created_at);

CREATE INDEX idx_messages_fts ON messages USING gin (search_vector);
CREATE INDEX idx_messages_conversation ON messages (conversation_id, created_at);
```

**Tier 5: Skill Memory (Procedural)**
- Auto-generated skill documents from completed tasks
- Agent reflects on task completion, writes reusable procedure
- Stored as structured records with markdown content
- Retrieved and improved on subsequent similar tasks

```sql
CREATE TABLE skills (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    content TEXT NOT NULL, -- markdown skill document
    content_embedding vector(1536), -- for semantic skill matching
    category TEXT,
    author TEXT NOT NULL, -- 'system', 'user:{id}', or 'agent:{id}'
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
```

**Tier 6: User Model**
- Progressive understanding of each user built over time
- Communication preferences, expertise level, work patterns
- Updated after each conversation via background task

```sql
CREATE TABLE user_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL UNIQUE REFERENCES users(id),
    profile_data JSONB NOT NULL DEFAULT '{}', -- preferences, expertise, patterns
    profile_embedding vector(1536), -- for matching similar users / recommendations
    last_updated TIMESTAMPTZ DEFAULT now(),
    update_count INTEGER DEFAULT 0
);
```

#### Memory Operations

The memory system exposes these operations to agents via tools:

| Operation | Description |
|---|---|
| `memory.store(content, scope, category)` | Store a new memory with auto-generated embedding |
| `memory.search(query, scope, limit)` | Hybrid search: FTS + vector similarity, ranked by relevance |
| `memory.recall(conversation_id)` | Recall a specific past conversation with summary |
| `memory.forget(memory_id)` | Mark memory for deletion (respects retention policies) |
| `memory.update(memory_id, content)` | Update an existing memory, re-embed |
| `memory.search_history(query, user_id)` | FTS + semantic search across all conversation history |

#### Hybrid Search Implementation

```sql
-- Hybrid search: combines FTS rank + vector cosine similarity
-- Uses Reciprocal Rank Fusion (RRF) for score merging
WITH fts_results AS (
    SELECT id, content, ts_rank(search_vector, plainto_tsquery('english', $1)) AS fts_score
    FROM memories
    WHERE search_vector @@ plainto_tsquery('english', $1)
      AND user_id = $2 AND scope = $3 AND NOT is_deleted
    ORDER BY fts_score DESC LIMIT 20
),
vector_results AS (
    SELECT id, content, 1 - (embedding <=> $4::vector) AS vec_score
    FROM memories
    WHERE user_id = $2 AND scope = $3 AND NOT is_deleted
    ORDER BY embedding <=> $4::vector LIMIT 20
),
combined AS (
    SELECT
        COALESCE(f.id, v.id) AS id,
        COALESCE(f.content, v.content) AS content,
        COALESCE(1.0 / (60 + RANK() OVER (ORDER BY f.fts_score DESC NULLS LAST)), 0) AS fts_rrf,
        COALESCE(1.0 / (60 + RANK() OVER (ORDER BY v.vec_score DESC NULLS LAST)), 0) AS vec_rrf
    FROM fts_results f FULL OUTER JOIN vector_results v ON f.id = v.id
)
SELECT id, content, (fts_rrf + vec_rrf) AS combined_score
FROM combined
ORDER BY combined_score DESC
LIMIT $5;
```

#### Pluggable Memory Backends

The memory system is built on a `MemoryBackend` trait. PostgreSQL + pgvector is the default implementation, but users can plug in external memory systems — either as a replacement or as an additional layer.

```rust
#[async_trait]
pub trait MemoryBackend: Send + Sync {
    /// Store a memory with optional embedding
    async fn store(&self, memory: &MemoryRecord) -> Result<String>; // returns ID

    /// Hybrid search: text + semantic similarity
    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        scope: MemoryScope,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryResult>>;

    /// Retrieve a specific memory by ID
    async fn get(&self, id: &str) -> Result<Option<MemoryRecord>>;

    /// Update an existing memory
    async fn update(&self, id: &str, content: &str, embedding: Option<&[f32]>) -> Result<()>;

    /// Soft-delete a memory (retention-aware)
    async fn delete(&self, id: &str) -> Result<()>;

    /// Hard-delete all data for a user (GDPR erasure)
    async fn purge_user(&self, user_id: &str) -> Result<u64>; // returns count deleted

    /// Health check
    async fn health(&self) -> Result<bool>;
}
```

**Built-in implementations:**

| Backend | Description | When to use |
|---|---|---|
| `PostgresMemory` (default) | pgvector + tsvector hybrid search | Default for all deployments, scales to millions of memories |
| `QdrantMemory` | Qdrant vector database | When you need billions of embeddings or advanced filtering |
| `PineconeMemory` | Pinecone managed vector DB | Serverless, zero-ops vector search at scale |
| `WeaviateMemory` | Weaviate vector + graph | When you need graph-aware semantic search |
| `ChromaMemory` | ChromaDB | Lightweight, local-first, good for development |

**Configuration:**

```yaml
memory:
  # Primary backend — handles all memory operations
  backend: "postgres"  # postgres | qdrant | pinecone | weaviate | chroma

  # Backend-specific config
  postgres:
    # Uses the main database.postgres_url by default

  qdrant:
    url: "http://localhost:6333"
    collection: "agent_memories"
    api_key: "${QDRANT_API_KEY}"

  pinecone:
    api_key: "${PINECONE_API_KEY}"
    environment: "us-east-1"
    index: "agent-memories"

  weaviate:
    url: "http://localhost:8081"
    api_key: "${WEAVIATE_API_KEY}"

  chroma:
    path: "~/.agentplatform/data/chroma"

  # Optional: layered memory — use multiple backends
  # Primary handles writes; all backends are searched and results merged
  layers:
    - backend: "postgres"      # structured data + FTS (always present)
      role: "primary"
    - backend: "qdrant"        # high-scale vector search
      role: "vector"
    - backend: "custom"        # user's own knowledge system
      role: "supplemental"
      url: "http://my-knowledge-api:8000"  # any API implementing MemoryBackend

  embedding_model: "openai/text-embedding-3-small"
  embedding_dimensions: 1536
  auto_memorize: true
  user_modeling: true
```

**Layered memory architecture:**

When multiple backends are configured as layers, the memory system works like this:

```
WRITE PATH:
  memory.store() → writes to primary backend (Postgres)
                 → async replicates embedding to vector layer (Qdrant)
                 → does NOT write to supplemental (read-only external knowledge)

SEARCH PATH:
  memory.search() → queries ALL layers in parallel
                  → merges results using Reciprocal Rank Fusion
                  → deduplicates by content similarity
                  → returns unified ranked results

DELETE PATH:
  memory.delete() → deletes from primary
                  → deletes from vector layer
                  → cannot delete from supplemental (external system manages its own data)
```

This means a user can connect their existing Qdrant instance, a corporate knowledge base, or any custom API that implements the `MemoryBackend` trait — and agents will seamlessly search across all of them alongside the platform's own memory.

**Custom external memory via HTTP:**

For users who don't want to write Rust, any HTTP API that implements these endpoints works as a `custom` backend:

```
POST   /memory          Store a memory (body: MemoryRecord JSON)
GET    /memory/search    Search (query params: q, embedding, scope, user_id, limit)
GET    /memory/:id       Get by ID
PUT    /memory/:id       Update
DELETE /memory/:id       Delete
DELETE /memory/user/:id  Purge all user data
GET    /health           Health check
```

### 3.2 Document Store

#### Built-In Document Workspace

```sql
CREATE TABLE workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id UUID REFERENCES organizations(id),
    name TEXT NOT NULL,
    description TEXT,
    connector_type TEXT DEFAULT 'local' CHECK (
        connector_type IN ('local', 'google_drive', 'sharepoint', 's3', 'dropbox')
    ),
    connector_config JSONB, -- encrypted connection details
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
    storage_key TEXT NOT NULL, -- S3/MinIO object key
    content_text TEXT, -- extracted text for search
    content_embedding vector(1536),
    metadata JSONB DEFAULT '{}', -- page count, author, extracted entities
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
```

Document files are stored in S3-compatible object storage (MinIO for self-hosted). Only metadata, extracted text, and embeddings live in PostgreSQL.

#### External Connectors (Phase 6)

Each connector implements a standard trait (Rust) / interface (TypeScript):

```rust
#[async_trait]
pub trait DocumentConnector: Send + Sync {
    async fn list_files(&self, path: &str) -> Result<Vec<FileInfo>>;
    async fn read_file(&self, file_id: &str) -> Result<Bytes>;
    async fn write_file(&self, path: &str, content: Bytes) -> Result<FileInfo>;
    async fn watch(&self, path: &str, tx: Sender<FileEvent>) -> Result<()>;
    async fn search(&self, query: &str) -> Result<Vec<FileInfo>>;
}
```

### 3.3 Audit Logger

Every action in the system is logged immutably. Built into the foundation layer from day one.

```sql
CREATE TABLE audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
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
    request_id UUID -- for correlating related events
) PARTITION BY RANGE (timestamp);

-- Partitioned by month for efficient retention management
-- DROP old partitions instead of DELETE — instant, no vacuum needed

-- Event types (enforced in application layer, not CHECK constraint for extensibility):
-- user.login, user.logout, user.created, user.updated, user.deleted
-- agent.message.sent, agent.message.received
-- agent.tool.called, agent.tool.result
-- agent.skill.created, agent.skill.improved, agent.skill.executed
-- agent.memory.stored, agent.memory.updated, agent.memory.deleted
-- agent.model.called, agent.model.response
-- agent.delegated, agent.delegation.result
-- document.uploaded, document.accessed, document.deleted
-- schedule.created, schedule.executed, schedule.failed
-- retention.policy.applied, retention.record.purged
-- retention.legal_hold.set, retention.legal_hold.released
-- admin.config.changed, admin.user.role_changed
-- system.startup, system.shutdown, system.error

CREATE INDEX idx_audit_timestamp ON audit_log (timestamp);
CREATE INDEX idx_audit_user ON audit_log (user_id, timestamp);
CREATE INDEX idx_audit_type ON audit_log (event_type, timestamp);
CREATE INDEX idx_audit_conversation ON audit_log (conversation_id) WHERE conversation_id IS NOT NULL;

-- CRITICAL: No UPDATE or DELETE operations on audit_log in application code
-- Retention handled by dropping partitions
```

### 3.4 Retention Engine

```sql
CREATE TABLE retention_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    applies_to TEXT NOT NULL CHECK (applies_to IN (
        'conversations', 'memories', 'documents', 'skills', 'audit_log'
    )),
    retention_days INTEGER, -- NULL = keep forever
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
    scope JSONB NOT NULL, -- {"users": [...], "conversations": [...], "date_range": {...}}
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
```

#### Retention Worker (runs on configurable schedule)

```
1. Find records past eligible_at with status = 'pending'
2. Check for legal holds — mark as 'held' if covered
3. If policy.requires_review → set status = 'under_review', notify admin
4. If auto-dispose → execute action (delete/archive/anonymize)
5. For 'delete': cascade across all tables + remove embeddings + purge from Redis
6. For 'archive': move to cold storage (S3 archive tier)
7. For 'anonymize': strip PII, keep aggregate data
8. Log every action to audit_log
9. For audit_log itself: DROP old monthly partitions (instant, no row-by-row delete)
```

#### GDPR Operations

| Operation | Implementation |
|---|---|
| Right to erasure | Cascade delete: user record → conversations → messages → memories → documents → skills → user_profile → audit_log anonymization |
| Data portability | Export all user data as JSON archive: conversations, memories, documents, skills, profile |
| Consent tracking | JSONB field on users table tracking consent grants/revocations with timestamps |

### 3.5 Configuration Management

All configuration via YAML files + environment variable substitution. No code required to configure agents, providers, or policies.

#### Directory Structure

```
~/.agentplatform/
├── config.yaml              # Main configuration
├── agents/
│   ├── supervisor.yaml      # Supervisor agent definition
│   ├── researcher.yaml      # Research agent definition
│   ├── writer.yaml          # Writing agent definition
│   └── ...
├── skills/
│   ├── built-in/            # Ships with platform
│   ├── community/           # Installed from marketplace
│   └── learned/             # Auto-generated by agents
├── data/                    # Local data (if not using external Postgres/Redis/S3)
│   ├── postgres/
│   ├── redis/
│   └── minio/
└── logs/
```

#### Main Configuration (config.yaml)

```yaml
platform:
  name: "My Agent Platform"
  host: "0.0.0.0"
  port: 8080
  secret_key: "${PLATFORM_SECRET_KEY}"

database:
  postgres_url: "${DATABASE_URL:-postgresql://platform:platform@localhost:5432/agentplatform}"
  redis_url: "${REDIS_URL:-redis://localhost:6379}"
  s3:
    endpoint: "${S3_ENDPOINT:-http://localhost:9000}"
    access_key: "${S3_ACCESS_KEY}"
    secret_key: "${S3_SECRET_KEY}"
    bucket: "${S3_BUCKET:-agentplatform}"

providers:
  default: "openrouter"
  openrouter:
    api_key: "${OPENROUTER_API_KEY}"
    default_model: "anthropic/claude-sonnet-4-20250514"
  anthropic:
    api_key: "${ANTHROPIC_API_KEY}"
    default_model: "claude-sonnet-4-20250514"
  openai:
    api_key: "${OPENAI_API_KEY}"
    default_model: "gpt-4o"
  ollama:
    base_url: "http://localhost:11434"
    default_model: "llama3.1:8b"

routing:
  strategy: "cost_optimized"  # cost_optimized | quality_first | speed_first | manual
  fallback_chain:
    - "anthropic/claude-sonnet-4-20250514"
    - "openai/gpt-4o"
    - "ollama/llama3.1:8b"
  cost_limit_daily_usd: 10.00
  cost_limit_monthly_usd: 200.00

memory:
  embedding_model: "openai/text-embedding-3-small"
  embedding_dimensions: 1536
  auto_memorize: true
  user_modeling: true
  context_compression:
    strategy: "summarize"  # summarize | sliding_window | hybrid
    max_context_tokens: 100000

scheduling:
  enabled: true
  heartbeat_interval_seconds: 60
  timezone: "America/Chicago"

retention:
  default_conversation_days: 365
  default_memory_days: null  # keep forever
  default_document_days: null
  audit_log_days: 2555  # 7 years
  auto_purge: true
  purge_schedule: "0 2 * * *"  # cron: daily at 2am

channels:
  web:
    enabled: true
  api:
    enabled: true
    rate_limit_per_minute: 100
  telegram:
    enabled: false
    bot_token: "${TELEGRAM_BOT_TOKEN}"
  discord:
    enabled: false
    bot_token: "${DISCORD_BOT_TOKEN}"
  slack:
    enabled: false
    bot_token: "${SLACK_BOT_TOKEN}"
    app_token: "${SLACK_APP_TOKEN}"
```

#### Agent Definition (agents/researcher.yaml)

```yaml
agent:
  id: "researcher"
  name: "Research Agent"
  description: "Searches the web, reads documents, and synthesizes findings"
  model: "anthropic/claude-sonnet-4-20250514"
  temperature: 0.3

  system_prompt: |
    You are a research agent. Your job is to find, analyze, and synthesize
    information from web sources and the document workspace.
    Always cite your sources. Prefer primary sources over aggregators.
    When you complete a research task, reflect on your process and create
    a skill document if the pattern is reusable.

  tools:
    - web_search
    - web_fetch
    - document_read
    - document_search
    - memory_search
    - memory_store
    - skill_create

  skills:
    - built-in/web-research
    - built-in/summarization

  max_context_tokens: 128000
  max_tool_calls_per_turn: 20
  self_improve: true
  memory_scope: "shared"
```

---

## 4. Agent Runtime (Layer 2)

### 4.1 ReAct Reasoning Loop

The core agent loop follows the ReAct (Reasoning + Acting) pattern. Implemented in Rust for performance and safety.

```rust
pub struct AgentRuntime {
    config: AgentConfig,
    model_router: ModelRouter,
    tool_registry: ToolRegistry,
    memory: MemoryService,
    skill_learner: SkillLearner,
    audit: AuditLogger,
}

impl AgentRuntime {
    pub async fn run(
        &self,
        message: &str,
        context: &mut ConversationContext,
    ) -> Result<impl Stream<Item = StreamChunk>> {
        // 1. Build prompt with system instructions, memory, conversation history
        let memories = self.memory.recall_relevant(message, context.user_id).await?;
        let skills = self.skill_learner.find_relevant(message).await?;
        let user_profile = self.memory.get_user_profile(context.user_id).await?;
        let prompt = PromptBuilder::build(&self.config, message, context, &memories, &skills, &user_profile);

        // 2. ReAct loop
        loop {
            let response = self.model_router.chat(&prompt, &self.tool_registry.schemas(), true).await?;

            // Stream text to caller
            // ... yield StreamChunk::Text for each token

            // 3. Handle tool calls
            if let Some(tool_calls) = response.tool_calls {
                for tc in &tool_calls {
                    self.audit.log(AuditEvent::ToolCalled {
                        tool: &tc.name,
                        input: &tc.input,
                        agent_id: &self.config.id,
                        conversation_id: context.conversation_id,
                    }).await;

                    let result = self.tool_registry.execute(tc).await?;

                    self.audit.log(AuditEvent::ToolResult {
                        tool: &tc.name,
                        output_summary: &result.summary(),
                        agent_id: &self.config.id,
                    }).await;

                    prompt.add_tool_result(tc.id, &result);
                }
                continue; // Loop back for LLM to reason about results
            } else {
                break; // No tool calls — response complete
            }
        }

        // 4. Post-response processing (spawned as background task)
        tokio::spawn(self.post_process(message, response, context));

        Ok(())
    }

    async fn post_process(&self, message: &str, response: &Response, context: &ConversationContext) {
        // Auto-extract and store memories
        if self.config.memory_scope.is_some() {
            self.memory.auto_extract(message, response, context).await;
        }
        // Update user profile
        self.memory.update_user_profile(context.user_id, message, response).await;
        // Self-improvement: reflect and create/update skills
        if self.config.self_improve {
            self.skill_learner.reflect(message, response, context).await;
        }
    }
}
```

### 4.2 Provider-Agnostic Model Routing

```rust
pub struct ModelRouter {
    providers: HashMap<String, Box<dyn LlmProvider>>,
    config: RoutingConfig,
    cost_tracker: CostTracker,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(
        &self,
        model: &str,
        messages: &[Message],
        tools: Option<&[ToolSchema]>,
        stream: bool,
    ) -> Result<LlmResponse>;

    fn supports_model(&self, model: &str) -> bool;
    fn cost_per_token(&self, model: &str) -> (f64, f64); // (input, output)
}

// Implementations: AnthropicProvider, OpenAIProvider, OllamaProvider, OpenRouterProvider
// All normalize to the same LlmResponse type
```

### 4.3 Self-Improving Skill Loop

After task completion, the agent reflects and generates reusable skills:

```
1. Agent completes a substantial task (not simple Q&A)
2. Check if similar skill exists (vector similarity on skill embeddings)
3. If exists: improve existing skill with new experience, increment version
4. If not: create new skill document with:
   - Title + description
   - Step-by-step procedure
   - Tools used and in what order
   - Edge cases and gotchas
   - Adaptation notes for similar tasks
5. Store skill with embedding for future retrieval
6. Track usage_count and success_rate over time
7. Skills with low success_rate flagged for review/deletion
```

### 4.4 Built-In Tools

| Category | Tools |
|---|---|
| Web | `web_search`, `web_fetch`, `web_browse` |
| Documents | `document_upload`, `document_read`, `document_search`, `document_list` |
| Memory | `memory_store`, `memory_search`, `memory_recall`, `memory_forget`, `search_history` |
| Skills | `skill_create`, `skill_list`, `skill_execute` |
| Filesystem | `file_read`, `file_write`, `file_list`, `shell_execute` (sandboxed) |
| Communication | `send_message` (to channels), `send_email` |
| Scheduling | `schedule_create`, `schedule_list`, `schedule_delete` |
| Code | `code_execute` (sandboxed Python/JS via Wasmtime or Docker) |
| Delegation | `delegate_to_agent`, `delegate_parallel` |
| MCP | Dynamic tools from connected MCP servers |

---

## 5. Orchestration & Scheduling (Layer 3)

### 5.1 Multi-Agent Orchestration

Supervisor pattern: a primary agent decides when to handle directly vs. delegate to specialized agents.

```rust
pub struct Orchestrator {
    agents: HashMap<String, AgentRuntime>,
    supervisor_id: String,
}

impl Orchestrator {
    pub async fn run(&self, message: &str, context: &mut ConversationContext) -> Result<Stream<StreamChunk>> {
        let supervisor = &self.agents[&self.supervisor_id];
        // Supervisor has access to DelegationTool which can call other agents
        supervisor.run(message, context).await
    }
}

// DelegationTool: exposed to supervisor agent
// delegate_to_agent(agent_id, task, context) → runs sub-agent, returns result
// delegate_parallel([{agent_id, task}, ...]) → runs multiple agents concurrently via tokio::join!
```

The supervisor sees all available agents and their descriptions in its system prompt, and decides whether to handle a request directly or delegate. Sub-agent results are returned as tool results that the supervisor synthesizes into a final response.

### 5.2 Scheduling & Heartbeat

```sql
CREATE TABLE schedules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    agent_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    cron_expression TEXT NOT NULL, -- "0 7 * * 1-5" = 7am weekdays
    natural_language TEXT, -- "every weekday morning at 7am"
    task_prompt TEXT NOT NULL,
    delivery_channel TEXT DEFAULT 'web',
    delivery_target TEXT, -- channel-specific target
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
    conversation_id UUID REFERENCES conversations(id),
    tokens_used INTEGER,
    cost_usd NUMERIC(10, 6),
    error TEXT
);
```

#### Heartbeat System

```
The heartbeat is a Tokio-based timer that runs independently of user interaction.

Every tick (configurable, default 60s):
1. Check for due scheduled tasks → spawn agent execution
2. Check monitored sources (file watchers, webhooks, email polling)
3. Run memory maintenance if due (summarize old conversations, clean expired memories)

Scheduled task execution:
1. Create a new conversation context linked to the schedule
2. Run the configured agent with the task prompt
3. Capture the response
4. Deliver via configured channel (web notification, email, webhook, Slack, etc.)
5. Log the run with status, tokens, cost
6. Update schedule.last_run_at and schedule.next_run_at
```

#### Event-Driven Triggers

```yaml
# Defined in agent config or schedule config
triggers:
  - type: "file_watch"
    path: "~/documents/incoming/"
    events: ["created", "modified"]
    agent: "document_processor"
    prompt: "Process the new file at {file_path}"

  - type: "webhook"
    path: "/hooks/github"
    agent: "developer"
    prompt: "Handle this GitHub event: {payload}"

  - type: "email"
    mailbox: "tasks@example.com"
    filter: "subject:ACTION REQUIRED"
    agent: "task_manager"
    prompt: "Handle this email: {email_body}"
```

---

## 6. Multi-User & Auth (Layer 4)

```sql
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
    password_hash TEXT NOT NULL, -- Argon2id
    role TEXT DEFAULT 'member' CHECK (role IN ('admin', 'member', 'viewer')),
    org_id UUID REFERENCES organizations(id),
    settings JSONB DEFAULT '{}', -- UI preferences, default model, timezone
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT now(),
    last_login_at TIMESTAMPTZ
);

CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    key_hash TEXT NOT NULL, -- Argon2id hash of the key
    key_prefix TEXT NOT NULL, -- first 8 chars for identification
    name TEXT,
    permissions JSONB DEFAULT '["*"]',
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT now()
);
```

### Role Permissions

| Permission | Admin | Member | Viewer |
|---|---|---|---|
| Chat with agents | Yes | Yes | Yes |
| View shared memory | Yes | Yes | Yes |
| Upload documents | Yes | Yes | No |
| Create schedules | Yes | Yes | No |
| Manage personal memory | Yes | Yes | No |
| View audit logs | Yes | Limited (own) | No |
| Manage agents | Yes | No | No |
| Manage users | Yes | No | No |
| Configure providers | Yes | No | No |
| Set retention policies | Yes | No | No |
| Manage legal holds | Yes | No | No |
| View cost reports | Yes | No | No |

---

## 7. Web UI (Layer 6)

### Views & Pages

| View | Description |
|---|---|
| **Dashboard** | Overview: active agents, recent conversations, upcoming schedules, usage stats, cost summary |
| **Agent Chat** | Primary chat interface: streaming responses, tool call visualization, delegation indicators, memory indicators |
| **Agent Selector** | Choose which agent to talk to, see descriptions and capabilities |
| **Memory Browser** | View, search, edit, delete stored memories across all tiers |
| **Skill Library** | Browse built-in, community, and auto-generated skills; enable/disable; see usage stats |
| **Document Workspace** | Upload, organize, search, manage documents; connector settings |
| **Schedule Manager** | Create, edit, monitor, review scheduled tasks and their results |
| **Audit & Compliance** | Search audit logs, manage retention policies, legal holds, GDPR tools |
| **Admin / Settings** | Provider config, user management, agent definitions, system settings |
| **Cost Dashboard** | Token usage, cost breakdown by model/agent/user, budget alerts |

### Real-Time Communication

- WebSocket connection per active user
- Server sends: text chunks, tool call starts/results, delegation events, memory events
- Client renders progressively: streaming text, tool call cards, sub-agent activity indicators
- Reconnection with message replay on disconnect

---

## 8. REST API (Layer 5)

```
POST   /api/v1/chat                     Send message, stream response (SSE or WebSocket)
GET    /api/v1/conversations             List conversations
GET    /api/v1/conversations/:id         Get conversation with messages
DELETE /api/v1/conversations/:id         Delete conversation (respects retention)

GET    /api/v1/agents                    List available agents
GET    /api/v1/agents/:id                Get agent details and capabilities

GET    /api/v1/memory                    Search memories
POST   /api/v1/memory                    Store memory
PUT    /api/v1/memory/:id                Update memory
DELETE /api/v1/memory/:id                Delete memory (respects retention)

GET    /api/v1/documents                 List documents
POST   /api/v1/documents                 Upload document
GET    /api/v1/documents/:id             Get document metadata
GET    /api/v1/documents/:id/download    Download document file
DELETE /api/v1/documents/:id             Delete document

GET    /api/v1/schedules                 List schedules
POST   /api/v1/schedules                 Create schedule
PUT    /api/v1/schedules/:id             Update schedule
DELETE /api/v1/schedules/:id             Delete schedule
GET    /api/v1/schedules/:id/runs        Get run history

GET    /api/v1/skills                    List skills
POST   /api/v1/skills                    Create/import skill
PUT    /api/v1/skills/:id                Update skill
DELETE /api/v1/skills/:id                Delete skill

GET    /api/v1/audit                     Search audit logs
GET    /api/v1/audit/export              Export audit logs

GET    /api/v1/usage                     Usage and cost data
GET    /api/v1/retention/policies         List retention policies
POST   /api/v1/retention/policies         Create retention policy
GET    /api/v1/retention/holds            List legal holds
POST   /api/v1/retention/holds            Create legal hold

POST   /api/v1/auth/login                Login (returns JWT)
POST   /api/v1/auth/register             Register (if enabled)
POST   /api/v1/auth/refresh              Refresh JWT
GET    /api/v1/auth/me                   Current user info
```

---

## 9. Implementation Phases

### Phase 1: Foundation + Single Agent (Weeks 1-6)

**Goal:** A working agent you can chat with in a browser, backed by PostgreSQL, with memory, audit logging, and document support. The foundation must be solid — this is the hardest phase.

**Deliverables:**
- [ ] Rust project scaffolding with Cargo workspace (core, api, cli crates)
- [ ] PostgreSQL schema: all tables from this document, with migrations
- [ ] Redis integration for session cache
- [ ] MinIO/S3 integration for document storage
- [ ] Config loader: YAML + env var substitution
- [ ] Model router: LLM provider trait + Anthropic, OpenAI, Ollama, OpenRouter implementations
- [ ] Single-agent ReAct loop with tool calling and streaming
- [ ] Memory system: store, hybrid search (FTS + pgvector), recall
- [ ] Audit logger: log every agent action to PostgreSQL
- [ ] Document upload + text extraction (PDF, DOCX, TXT, MD)
- [ ] Basic built-in tools: web_search, web_fetch, memory_store, memory_search, document_read
- [ ] TypeScript API layer: FastAPI-equivalent (Hono) with WebSocket endpoint
- [ ] React frontend: chat interface with streaming, conversation list, basic settings
- [ ] Docker Compose: postgres, redis, minio, rust-core, api, frontend
- [ ] Basic retention: conversation auto-delete after configurable days

### Phase 2: Intelligence (Weeks 7-10)

**Goal:** Multiple agents that coordinate, learn, and improve.

- [ ] Multi-agent orchestration: supervisor + delegate pattern
- [ ] Parallel agent execution (tokio::join!)
- [ ] Self-improving skill loop: reflect → create → retrieve → improve
- [ ] Skill library UI: built-in, auto-generated, enable/disable, usage stats
- [ ] User modeling: progressive profile built from conversations
- [ ] Episodic memory: search across all past conversations
- [ ] Agent definition via YAML (multiple agent configs)
- [ ] Tool call + delegation visualization in UI
- [ ] Memory browser UI

### Phase 3: Automation (Weeks 11-14)

**Goal:** Agents that work autonomously on schedules and triggers.

- [ ] Cron-based task scheduler (tokio-cron-scheduler)
- [ ] Natural language schedule creation
- [ ] Heartbeat system: periodic monitoring tick
- [ ] Event-driven triggers: file watch, webhook, email monitor
- [ ] Schedule manager UI: create, edit, monitor, review results
- [ ] Result delivery to channels (web notification, email, webhook)
- [ ] Schedule run history with conversation links

### Phase 4: Teams (Weeks 15-18)

**Goal:** Multiple users sharing a platform with appropriate access control.

- [ ] User registration and authentication (JWT + Argon2id)
- [ ] Role-based access control (admin, member, viewer)
- [ ] Organization/workspace model
- [ ] Per-user memory scope + shared org memory
- [ ] Per-user conversation isolation
- [ ] Per-user cost tracking and attribution
- [ ] User management admin UI
- [ ] API key management

### Phase 5: Compliance (Weeks 19-22)

**Goal:** Enterprise-grade data lifecycle management.

- [ ] Configurable retention policies (per workspace, per data type)
- [ ] Legal hold system (freeze specific records from deletion)
- [ ] Automated disposition workflow (review → approve → purge)
- [ ] GDPR: right to erasure (cascade delete + vector purge)
- [ ] GDPR: data portability (export all user data)
- [ ] Retention dashboard UI
- [ ] Audit log search and export UI

### Phase 6: Ecosystem (Weeks 23-26)

**Goal:** Connect to external systems and communication channels.

- [ ] Document connectors: Google Drive, SharePoint, S3
- [ ] Channel integrations: Telegram, Discord, Slack
- [ ] MCP server support
- [ ] Webhook system for custom integrations
- [ ] Connector management UI

### Phase 7: Polish & Launch (Weeks 27-30)

**Goal:** Production-ready, documented, and deployable.

- [ ] Polished UI with your chosen design direction
- [ ] Comprehensive documentation (setup guide, user guide, API docs)
- [ ] One-line install script
- [ ] Hosted version infrastructure
- [ ] Landing page and marketing site
- [ ] Security hardening review
- [ ] Load testing and performance optimization

---

## 10. Business Model

### Pricing (Hybrid Open Core)

**Open Source (AGPL-3.0 — free forever):**
- Full platform: all agents, memory, orchestration, scheduling, documents
- Multi-user with roles
- Basic retention (auto-delete policies)
- Audit logging
- Self-hosted deployment
- Community skills and support

**Pro — Commercial License ($29/user/month):**
- Everything in AGPL, plus:
- White-labeling / custom branding
- Priority support
- Managed hosting option

**Team — Commercial License ($59/user/month):**
- Everything in Pro, plus:
- SSO / SAML / OIDC
- Advanced retention workflows (review gates)
- Legal hold system
- Audit log export
- Custom branding

**Enterprise — Commercial License ($99/user/month, min 10 seats):**
- Everything in Team, plus:
- GDPR compliance toolkit (erasure, portability)
- Compliance reporting templates
- Dedicated support engineer
- On-premise deployment support
- SLA guarantee
- Custom integrations

---

## 11. Project Structure

```
agent-platform/
├── README.md
├── LICENSE                     # AGPL-3.0
├── docker-compose.yml
├── Dockerfile.core             # Rust core binary
├── Dockerfile.api              # TypeScript API
├── Dockerfile.frontend         # React frontend
│
├── core/                       # Rust workspace
│   ├── Cargo.toml              # Workspace root
│   ├── crates/
│   │   ├── runtime/            # Agent ReAct loop, tool execution
│   │   │   └── src/
│   │   │       ├── agent.rs
│   │   │       ├── prompt_builder.rs
│   │   │       ├── tool_registry.rs
│   │   │       └── stream.rs
│   │   ├── models/             # LLM provider trait + implementations
│   │   │   └── src/
│   │   │       ├── router.rs
│   │   │       ├── anthropic.rs
│   │   │       ├── openai.rs
│   │   │       ├── ollama.rs
│   │   │       └── openrouter.rs
│   │   ├── memory/             # Memory system (all tiers)
│   │   │   └── src/
│   │   │       ├── store.rs
│   │   │       ├── search.rs   # Hybrid FTS + vector
│   │   │       ├── episodic.rs
│   │   │       ├── user_profile.rs
│   │   │       └── compression.rs
│   │   ├── orchestrator/       # Multi-agent coordination
│   │   │   └── src/
│   │   │       ├── supervisor.rs
│   │   │       └── delegation.rs
│   │   ├── scheduler/          # Cron, heartbeat, triggers
│   │   │   └── src/
│   │   │       ├── cron.rs
│   │   │       ├── heartbeat.rs
│   │   │       └── triggers.rs
│   │   ├── skills/             # Skill learner + executor
│   │   │   └── src/
│   │   │       ├── learner.rs
│   │   │       ├── executor.rs
│   │   │       └── registry.rs
│   │   ├── documents/          # Document store + extractors
│   │   │   └── src/
│   │   │       ├── store.rs
│   │   │       ├── extractors.rs
│   │   │       └── connectors/
│   │   ├── retention/          # Retention engine
│   │   │   └── src/
│   │   │       ├── engine.rs
│   │   │       ├── policies.rs
│   │   │       └── worker.rs
│   │   ├── audit/              # Audit logger
│   │   │   └── src/
│   │   │       └── logger.rs
│   │   ├── auth/               # Auth + permissions
│   │   │   └── src/
│   │   │       ├── jwt.rs
│   │   │       ├── argon2.rs
│   │   │       └── permissions.rs
│   │   ├── tools/              # Built-in agent tools
│   │   │   └── src/
│   │   │       ├── web.rs
│   │   │       ├── documents.rs
│   │   │       ├── memory.rs
│   │   │       ├── filesystem.rs
│   │   │       ├── code_exec.rs
│   │   │       ├── scheduling.rs
│   │   │       └── delegation.rs
│   │   ├── config/             # YAML config loader
│   │   │   └── src/
│   │   │       └── lib.rs
│   │   ├── security/           # Sandbox, permissions, webhook verification
│   │   │   └── src/
│   │   │       ├── sandbox.rs      # Wasmtime/Docker code execution sandbox
│   │   │       ├── permissions.rs  # Agent permission enforcement
│   │   │       ├── webhook.rs      # HMAC signature verification
│   │   │       └── secrets.rs      # Secrets loading (env, encrypted file, Vault)
│   │   ├── observability/      # Metrics, tracing, health checks
│   │   │   └── src/
│   │   │       ├── metrics.rs      # Prometheus metrics
│   │   │       ├── tracing.rs      # OpenTelemetry distributed tracing
│   │   │       └── health.rs       # Health check endpoints
│   │   └── db/                 # Database layer (sqlx)
│   │       └── src/
│   │           ├── postgres.rs
│   │           ├── redis.rs
│   │           ├── s3.rs
│   │           └── migrations/
│   └── src/
│       └── main.rs             # Core binary entry point (HTTP server)
│
├── api/                        # TypeScript API gateway
│   ├── package.json
│   ├── src/
│   │   ├── index.ts
│   │   ├── routes/
│   │   │   ├── chat.ts
│   │   │   ├── conversations.ts
│   │   │   ├── memory.ts
│   │   │   ├── documents.ts
│   │   │   ├── schedules.ts
│   │   │   ├── skills.ts
│   │   │   ├── audit.ts
│   │   │   ├── auth.ts
│   │   │   └── users.ts
│   │   ├── websocket/
│   │   │   └── handler.ts
│   │   └── types/
│   │       └── shared.ts       # Types shared with frontend
│   └── tsconfig.json
│
├── frontend/                   # React frontend
│   ├── package.json
│   ├── src/
│   │   ├── App.tsx
│   │   ├── components/
│   │   │   ├── Chat/
│   │   │   ├── Dashboard/
│   │   │   ├── Memory/
│   │   │   ├── Documents/
│   │   │   ├── Schedules/
│   │   │   ├── Skills/
│   │   │   ├── Audit/
│   │   │   ├── Admin/
│   │   │   └── shared/
│   │   ├── hooks/
│   │   ├── stores/             # Zustand
│   │   ├── api/                # API client (typed)
│   │   └── types/
│   └── tsconfig.json
│
├── migrations/                 # SQL migrations
│   ├── 001_initial_schema.sql
│   └── ...
│
├── skills/                     # Built-in skill documents
│   ├── web-research/
│   ├── summarization/
│   └── document-analysis/
│
├── config.example.yaml
│
└── tests/
    ├── core/                   # Rust integration tests
    ├── api/                    # API endpoint tests
    └── e2e/                    # End-to-end tests
```

---

## 12. Key Dependencies

### Rust (core/)

```toml
[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
axum = "0.7"                    # HTTP framework (or actix-web)
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }
redis = { version = "0.27", features = ["tokio-comp", "streams"] }
aws-sdk-s3 = "1"               # S3-compatible object storage
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
reqwest = { version = "0.12", features = ["json", "stream"] }
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
argon2 = "0.5"                  # Password hashing (PHC winner)
jsonwebtoken = "9"              # JWT
tokio-cron-scheduler = "0.13"   # Cron scheduling
tracing = "0.1"                 # Structured logging
tracing-subscriber = "0.3"
tracing-opentelemetry = "0.28"  # Distributed tracing
opentelemetry = "0.27"          # OpenTelemetry SDK
opentelemetry-otlp = "0.27"    # OTLP exporter (Jaeger, Tempo, etc.)
metrics = "0.24"                # Prometheus metrics
metrics-exporter-prometheus = "0.16"
async-trait = "0.1"
futures = "0.3"
anyhow = "1"
thiserror = "2"
pgvector = "0.4"                # pgvector types for sqlx
hmac = "0.12"                   # HMAC for webhook signature verification
sha2 = "0.10"                   # SHA-256 for signatures
subtle = "2"                    # Constant-time comparison
wasmtime = "28"                 # WebAssembly sandbox for code execution
bollard = "0.18"                # Docker API client (alternative sandbox)
age = "0.10"                    # File encryption for secrets

[workspace.dev-dependencies]
testcontainers = "0.23"         # Postgres/Redis in Docker for tests
testcontainers-modules = { version = "0.11", features = ["postgres", "redis"] }
wiremock = "0.6"                # HTTP mocking for LLM provider tests
tokio-test = "0.4"
assert_matches = "1"
```

### TypeScript (api/ and frontend/)

```json
{
  "api_dependencies": {
    "hono": "^4",
    "ws": "^8",
    "zod": "^3"
  },
  "frontend_dependencies": {
    "react": "^19",
    "react-dom": "^19",
    "typescript": "^5.5",
    "tailwindcss": "^4",
    "@tanstack/react-query": "^5",
    "zustand": "^5",
    "react-markdown": "^9",
    "lucide-react": "^0.400"
  }
}
```

---

## 13. Docker Compose

```yaml
version: "3.9"

services:
  postgres:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_DB: agentplatform
      POSTGRES_USER: platform
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - postgres_data:/var/lib/postgresql/data
    ports:
      - "5432:5432"

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    volumes:
      - redis_data:/data

  minio:
    image: minio/minio
    command: server /data --console-address ":9001"
    environment:
      MINIO_ROOT_USER: ${S3_ACCESS_KEY}
      MINIO_ROOT_PASSWORD: ${S3_SECRET_KEY}
    volumes:
      - minio_data:/data
    ports:
      - "9000:9000"
      - "9001:9001"

  core:
    build:
      context: .
      dockerfile: Dockerfile.core
    environment:
      DATABASE_URL: postgresql://platform:${DB_PASSWORD}@postgres:5432/agentplatform
      REDIS_URL: redis://redis:6379
      S3_ENDPOINT: http://minio:9000
      S3_ACCESS_KEY: ${S3_ACCESS_KEY}
      S3_SECRET_KEY: ${S3_SECRET_KEY}
    depends_on:
      - postgres
      - redis
      - minio
    ports:
      - "9090:9090"  # Internal API (gRPC or HTTP)

  api:
    build:
      context: .
      dockerfile: Dockerfile.api
    environment:
      CORE_URL: http://core:9090
      PLATFORM_SECRET_KEY: ${PLATFORM_SECRET_KEY}
    depends_on:
      - core
    ports:
      - "8080:8080"

  frontend:
    build:
      context: .
      dockerfile: Dockerfile.frontend
    depends_on:
      - api
    ports:
      - "3000:3000"

volumes:
  postgres_data:
  redis_data:
  minio_data:
```

---

## 14. Security & Sandboxing

Agents can execute code, run shell commands, browse the web, and send emails. This is an enormous attack surface. OpenClaw accumulated 9+ CVEs in its first two months, and 42,665 instances were found exposed on the public internet. Security is not a Phase 5 feature — it's a Phase 1 requirement.

### 14.1 Code Execution Sandboxing

Every `code_execute` and `shell_execute` tool call runs inside an isolated sandbox. Never on the host.

**Sandbox options (choose one per deployment):**

| Backend | Isolation Level | Startup Time | Best For |
|---|---|---|---|
| Wasmtime (WASI) | Process-level, capability-based | <10ms | Python/JS code snippets, fastest |
| Docker containers | OS-level, namespace isolation | ~500ms | Full environment access, shell commands |
| Firecracker microVMs | Hardware-level | ~125ms | Maximum isolation, enterprise/government |
| gVisor (runsc) | Syscall filtering | ~200ms | Balance of speed and isolation |

**Sandbox policy (enforced by the runtime, not the agent):**

```yaml
sandbox:
  backend: "docker"  # wasmtime | docker | firecracker | gvisor
  limits:
    max_execution_seconds: 30
    max_memory_mb: 512
    max_cpu_cores: 1
    max_disk_mb: 100
    max_output_bytes: 1048576  # 1MB
  network:
    enabled: false              # default: no network access
    allow_list: []              # if enabled, only these domains
  filesystem:
    read_only_root: true
    writable_paths:
      - "/tmp"
      - "/workspace"            # agent working directory
    mount_documents: false      # must be explicitly enabled per agent
  capabilities:
    allow_shell: false          # must be explicitly enabled per agent
    allow_network: false
    allow_file_write: false
```

**Key rules:**
- Sandbox config is set by the admin, not the agent. Agents cannot escalate their own permissions.
- Every sandbox invocation is logged to the audit trail with input, output, exit code, and resource usage.
- Sandbox containers are ephemeral — destroyed after each execution. No persistent state between tool calls unless explicitly mounted.
- Timeout enforcement is hard — the sandbox runtime kills the process, not the agent.

### 14.2 Network Security

```yaml
security:
  # API layer
  cors:
    allowed_origins: ["https://your-domain.com"]
    allowed_methods: ["GET", "POST", "PUT", "DELETE"]
  
  # Rate limiting (see Section 15)
  
  # TLS
  tls:
    enabled: true
    cert_path: "/etc/certs/cert.pem"
    key_path: "/etc/certs/key.pem"
  
  # WebSocket
  websocket:
    require_auth: true
    max_connections_per_user: 5
    ping_interval_seconds: 30
    
  # Internal service communication
  internal:
    require_auth: true          # core ↔ api communication authenticated
    shared_secret: "${INTERNAL_SECRET}"
```

### 14.3 Agent Permissions Model

Each agent definition includes a permissions block that constrains what tools it can access:

```yaml
# agents/researcher.yaml
agent:
  id: "researcher"
  permissions:
    tools:
      allowed:
        - web_search
        - web_fetch
        - document_read
        - document_search
        - memory_search
        - memory_store
      denied:
        - shell_execute        # researcher cannot run shell commands
        - file_write           # researcher cannot write to filesystem
        - send_email           # researcher cannot send emails
    sandbox:
      network: true            # researcher needs web access
      file_write: false
    delegation:
      can_delegate: false      # only supervisor can delegate
    cost:
      max_tokens_per_turn: 50000
      max_cost_per_turn_usd: 0.50
```

---

## 15. Plugin & Skill Security

### 15.1 Skill Trust Levels

```
┌─────────────────────────────────────────────┐
│  BUILT-IN (ships with platform)             │
│  Trust: Full                                │
│  Review: Core team, signed                  │
├─────────────────────────────────────────────┤
│  VERIFIED (marketplace, reviewed)           │
│  Trust: High                                │
│  Review: Automated scan + manual review     │
├─────────────────────────────────────────────┤
│  COMMUNITY (marketplace, unreviewed)        │
│  Trust: Medium                              │
│  Review: Automated scan only                │
├─────────────────────────────────────────────┤
│  AUTO-GENERATED (created by agents)         │
│  Trust: Medium                              │
│  Review: None — limited to agent's own perms│
├─────────────────────────────────────────────┤
│  USER-UPLOADED (sideloaded)                 │
│  Trust: Low                                 │
│  Review: None — admin must explicitly enable│
└─────────────────────────────────────────────┘
```

### 15.2 Skill Manifest

Every skill declares what it needs. The runtime enforces these declarations.

```yaml
# skills/my-skill/SKILL.yaml
skill:
  name: "web-research"
  version: "1.0.0"
  description: "Search the web and synthesize findings"
  author: "platform-team"
  
  # Permission declarations — what this skill needs to function
  permissions:
    network: true              # needs internet access
    filesystem: false          # does not need file access
    shell: false               # does not need shell
    tools_required:
      - web_search
      - web_fetch
    tools_optional:
      - memory_store
  
  # Signature (for verified/built-in skills)
  signature: "sha256:abc123..."
  signed_by: "platform-team"
```

### 15.3 Marketplace Security Pipeline

```
Skill submitted → Automated scan:
  1. Static analysis: no hardcoded URLs, no obfuscated code, no eval()
  2. Permission audit: declared permissions match actual tool usage
  3. Dependency check: no known-vulnerable dependencies
  4. Data flow analysis: no data exfiltration patterns (sending memory/conversation data to external endpoints)
  5. Size limits: max 1MB per skill
→ If community: published with warning badge
→ If verified: manual review by maintainer → signed → published
```

---

## 16. Rate Limiting & Abuse Prevention

### 16.1 Multi-Layer Rate Limiting

```yaml
rate_limiting:
  # API layer (per user)
  api:
    requests_per_minute: 60
    requests_per_hour: 500
    burst: 20                  # max burst above sustained rate
  
  # LLM calls (per user)
  llm:
    requests_per_minute: 20
    tokens_per_minute: 100000
    tokens_per_hour: 500000
  
  # Tool execution (per user)
  tools:
    executions_per_minute: 30
    code_executions_per_hour: 50
    web_fetches_per_hour: 200
  
  # Document uploads (per user)
  documents:
    uploads_per_hour: 50
    max_file_size_mb: 100
    storage_quota_gb: 10       # per user
  
  # WebSocket (per user)
  websocket:
    messages_per_minute: 30
    max_concurrent_conversations: 5
```

### 16.2 Budget Enforcement

```sql
-- Real-time cost tracking in Redis, persisted to Postgres
-- Redis keys: "cost:{user_id}:daily", "cost:{user_id}:monthly"
-- Checked BEFORE every LLM call, not after

CREATE TABLE budget_limits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    scope TEXT NOT NULL CHECK (scope IN ('user', 'org', 'agent', 'global')),
    scope_id UUID, -- user_id, org_id, or NULL for global
    limit_daily_usd NUMERIC(10, 2),
    limit_monthly_usd NUMERIC(10, 2),
    action_on_limit TEXT DEFAULT 'block' CHECK (
        action_on_limit IN ('block', 'downgrade_model', 'notify_admin', 'notify_user')
    ),
    downgrade_model TEXT, -- model to fall back to when limit hit
    created_at TIMESTAMPTZ DEFAULT now()
);
```

**Enforcement flow:**
```
1. User sends message
2. Check Redis: current daily/monthly spend for user
3. Check budget_limits for user, org, and global
4. If approaching limit (80%): notify user inline ("You've used 80% of your daily budget")
5. If at limit:
   - 'block': return error "Daily budget exceeded. Resets at midnight."
   - 'downgrade_model': switch to cheaper model, inform user
   - 'notify_admin': allow but alert admin
6. After LLM response: update Redis counters, async persist to Postgres
```

---

## 17. Observability

### 17.1 Structured Logging

All logs are structured JSON via the `tracing` crate (Rust) — not println, not unstructured text.

```rust
// Every log includes these fields automatically:
// timestamp, level, service, request_id, user_id (if authenticated)

tracing::info!(
    user_id = %context.user_id,
    agent_id = %config.id,
    model = %model_used,
    tokens_in = response.usage.input,
    tokens_out = response.usage.output,
    latency_ms = elapsed.as_millis(),
    "Agent turn completed"
);
```

### 17.2 Metrics (OpenTelemetry / Prometheus)

```yaml
# Exported at /metrics (Prometheus format)

# Agent performance
agent_turns_total{agent_id, status}              # counter
agent_turn_duration_seconds{agent_id}             # histogram
agent_tool_calls_total{agent_id, tool}            # counter
agent_tool_duration_seconds{agent_id, tool}       # histogram
agent_delegations_total{supervisor, delegate}     # counter

# LLM provider metrics
llm_requests_total{provider, model, status}       # counter
llm_request_duration_seconds{provider, model}     # histogram
llm_tokens_total{provider, model, direction}      # counter (direction: input/output)
llm_cost_usd_total{provider, model}               # counter
llm_rate_limit_hits_total{provider}               # counter
llm_fallback_total{from_provider, to_provider}    # counter

# Memory system
memory_operations_total{operation, backend}       # counter
memory_search_duration_seconds{backend}           # histogram
memory_store_size_bytes{tier}                     # gauge

# Scheduling
schedule_runs_total{schedule_id, status}           # counter
schedule_run_duration_seconds{schedule_id}         # histogram
heartbeat_tick_duration_seconds                    # histogram

# System health
active_websocket_connections                       # gauge
active_conversations                               # gauge
database_query_duration_seconds{query_type}        # histogram
redis_operation_duration_seconds{operation}         # histogram
```

### 17.3 Distributed Tracing

Multi-agent delegation chains need end-to-end tracing. When agent A delegates to agent B which calls a tool that fails, you need to see the full trace.

```
Trace: user_message_abc123
├── Span: supervisor.run (agent: supervisor, 2.3s)
│   ├── Span: prompt_builder.build (0.05s)
│   ├── Span: llm.chat (provider: anthropic, model: claude-sonnet, 1.2s)
│   ├── Span: delegate_to_agent (agent: researcher, 0.8s)
│   │   ├── Span: researcher.run (0.8s)
│   │   │   ├── Span: llm.chat (provider: openai, model: gpt-4o, 0.4s)
│   │   │   ├── Span: tool.web_search (0.2s)
│   │   │   └── Span: tool.web_fetch (0.15s) [ERROR: timeout]
│   │   └── Span: skill_learner.reflect (0.05s)
│   └── Span: llm.chat (provider: anthropic, final synthesis, 0.2s)
└── Span: post_process (async, 0.1s)
    ├── Span: memory.auto_extract (0.05s)
    └── Span: memory.update_user_profile (0.05s)
```

**Implementation:** OpenTelemetry SDK for Rust (`opentelemetry`, `tracing-opentelemetry`). Export to Jaeger, Grafana Tempo, or any OTLP-compatible backend. For self-hosted users who don't want a separate tracing backend, traces are also written to the audit log with correlation IDs.

### 17.4 Health & Status Dashboard

```
GET /health          # simple up/down for load balancers
GET /health/detailed # full status of all subsystems

Response:
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_seconds": 86400,
  "subsystems": {
    "postgres": { "status": "healthy", "latency_ms": 2 },
    "redis": { "status": "healthy", "latency_ms": 1 },
    "s3": { "status": "healthy", "latency_ms": 15 },
    "scheduler": { "status": "healthy", "next_tick_in_seconds": 42 },
    "providers": {
      "anthropic": { "status": "healthy", "last_call_ms": 1200 },
      "ollama": { "status": "unhealthy", "error": "connection refused" }
    }
  }
}
```

---

## 18. Backup & Disaster Recovery

### 18.1 PostgreSQL Backup Strategy

```yaml
backup:
  postgres:
    # Continuous WAL archiving for point-in-time recovery
    wal_archiving:
      enabled: true
      destination: "s3://backups/wal/"
      retention_days: 30
    
    # Full base backups
    full_backup:
      schedule: "0 3 * * *"    # daily at 3am
      destination: "s3://backups/full/"
      retention_count: 30       # keep 30 daily backups
      tool: "pg_basebackup"     # or pgBackRest for production
    
    # Logical backups (for migration/portability)
    logical_backup:
      schedule: "0 4 * * 0"    # weekly on Sunday at 4am
      destination: "s3://backups/logical/"
      retention_count: 12       # keep 12 weekly backups
      tool: "pg_dump"
```

**Recovery targets:**
- RPO (Recovery Point Objective): < 1 minute (continuous WAL archiving)
- RTO (Recovery Time Objective): < 30 minutes (base backup + WAL replay)

### 18.2 Redis Persistence

```yaml
# Redis config
appendonly: yes
appendfsync: everysec          # fsync every second — good balance
aof-use-rdb-preamble: yes      # hybrid RDB+AOF for faster restarts
```

Redis is a cache layer. If Redis dies, the system rebuilds hot caches from PostgreSQL on next access. No data loss — just temporary slowness.

### 18.3 Object Storage (S3/MinIO)

- S3 versioning enabled on the documents bucket — accidental deletions are recoverable
- Cross-region replication for hosted/enterprise deployments
- MinIO self-hosted: scheduled `mc mirror` to backup destination

### 18.4 Disaster Recovery Runbook

```
1. Database corruption or loss:
   → Stop core service
   → Restore latest base backup from S3
   → Replay WAL logs to desired point in time
   → Verify data integrity
   → Restart core service
   → Redis caches rebuild automatically on demand

2. Complete infrastructure loss:
   → Provision new infrastructure (Docker Compose or Kubernetes)
   → Restore Postgres from backup
   → Restore MinIO data from backup
   → Redis starts empty (rebuilds from Postgres)
   → Verify config.yaml and secrets
   → Start services

3. Accidental data deletion by user:
   → If within retention period: restore from soft-delete (is_deleted flag)
   → If past retention: restore from Postgres PITR to before deletion
   → If document: restore from S3 versioning
```

---

## 19. Embedding Model Migration

Vector embeddings are tied to the model that created them. When a better embedding model becomes available (different dimensions, better quality), you need a re-embedding pipeline.

### 19.1 Schema Design for Migration

```sql
-- Every table with embeddings tracks the model that created them
ALTER TABLE memories ADD COLUMN embedding_model TEXT DEFAULT 'openai/text-embedding-3-small';
ALTER TABLE documents ADD COLUMN embedding_model TEXT DEFAULT 'openai/text-embedding-3-small';
ALTER TABLE skills ADD COLUMN embedding_model TEXT DEFAULT 'openai/text-embedding-3-small';
ALTER TABLE conversations ADD COLUMN summary_embedding_model TEXT DEFAULT 'openai/text-embedding-3-small';
```

### 19.2 Migration Pipeline

```
1. Admin sets new embedding model in config:
   memory.embedding_model: "openai/text-embedding-3-large"
   memory.embedding_dimensions: 3072

2. Migration job runs as background task:
   → Iterate all records with embedding_model != current model
   → Re-embed content using new model
   → Store new embedding alongside old (dual-write period)
   → Update embedding_model field
   → Batch size: 100 records per cycle, rate-limited to avoid API cost spikes

3. Once migration complete:
   → Drop old HNSW index
   → Rebuild HNSW index on new embeddings
   → Remove old embedding column (if dimensions changed)

4. Cost estimate shown to admin before starting:
   → "Re-embedding 145,000 records at $0.02/1M tokens ≈ $X.XX"
   → Admin confirms before proceeding
```

### 19.3 Dimension-Flexible Schema (Future-Proofing)

```sql
-- Instead of fixed vector(1536), use a dimension from config
-- pgvector supports this — just change the column type during migration
-- The HNSW index must be rebuilt when dimensions change
-- During migration: old and new embeddings coexist in separate columns
```

---

## 20. Webhook Security

All inbound webhooks (event triggers, channel integrations, external systems) are secured:

### 20.1 HMAC Signature Verification

```rust
pub fn verify_webhook(
    payload: &[u8],
    signature: &str,       // X-Webhook-Signature header
    secret: &str,          // shared secret per webhook source
    tolerance_seconds: u64, // max age of signature (default: 300)
) -> Result<bool> {
    // 1. Parse signature header: "t=timestamp,v1=hash"
    // 2. Verify timestamp is within tolerance (prevents replay attacks)
    // 3. Compute HMAC-SHA256(secret, "timestamp.payload")
    // 4. Compare computed hash with provided hash (constant-time comparison)
}
```

### 20.2 Webhook Configuration

```yaml
triggers:
  - type: "webhook"
    path: "/hooks/github"
    secret: "${GITHUB_WEBHOOK_SECRET}"
    signature_header: "X-Hub-Signature-256"
    verify: true               # HMAC verification required
    allowed_ips:               # optional IP allowlist
      - "140.82.112.0/20"     # GitHub's IP range
    agent: "developer"
    prompt: "Handle this GitHub event: {payload}"
```

### 20.3 Outbound Webhook Security

When the platform sends webhooks (schedule results, notifications):
- Signed with HMAC-SHA256 using a per-destination secret
- Sent over HTTPS only
- Retry with exponential backoff (max 5 attempts)
- Delivery status logged to audit trail

---

## 21. Secrets Management

### 21.1 Development / Self-Hosted (Simple)

Environment variables + `.env` file. Config.yaml references them via `${VAR_NAME}` substitution. The `.env` file is never committed to version control.

```bash
# .env (never committed)
PLATFORM_SECRET_KEY=your-random-32-char-string
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
DB_PASSWORD=strong-password-here
S3_ACCESS_KEY=minioadmin
S3_SECRET_KEY=minioadmin
```

### 21.2 Production / Teams (Encrypted Config)

```yaml
secrets:
  backend: "encrypted_file"   # encrypted_file | vault | aws_secrets_manager | azure_key_vault
  
  encrypted_file:
    path: "~/.agentplatform/secrets.enc"
    # Encrypted with age (https://age-encryption.org) or SOPS
    # Decrypted at startup using PLATFORM_MASTER_KEY env var
  
  vault:
    url: "https://vault.example.com"
    auth_method: "token"       # token | approle | kubernetes
    secret_path: "secret/data/agentplatform"
  
  aws_secrets_manager:
    region: "us-east-1"
    secret_name: "agentplatform/production"
  
  azure_key_vault:
    vault_url: "https://my-vault.vault.azure.net"
```

### 21.3 Secrets Handling Rules

- API keys are never logged, never included in audit trail details, never returned in API responses
- API keys stored in database (e.g., user-provided provider keys) are encrypted at rest using AES-256-GCM with a key derived from PLATFORM_MASTER_KEY
- Key rotation: platform generates a new JWT signing key on admin command; old tokens remain valid until expiry
- Provider API keys: users can rotate via settings UI; old key is overwritten immediately

---

## 22. Testing Strategy

### 22.1 Test Pyramid

```
                    ┌──────────┐
                    │   E2E    │  5-10 critical user journeys
                    │  tests   │  Playwright or similar
                   ┌┴──────────┴┐
                   │ Integration │  API endpoints, WebSocket flows
                   │   tests     │  Real Postgres (testcontainers)
                  ┌┴─────────────┴┐
                  │   Unit tests   │  Pure logic: prompt building,
                  │                │  memory scoring, cost calculation,
                  │                │  config parsing, permission checks
                  └────────────────┘
```

### 22.2 Testing Requirements by Layer

**Rust core (unit + integration):**
- Agent ReAct loop: mock LLM responses → verify tool calls are dispatched → verify streaming output
- Memory system: real Postgres via `testcontainers` → store/search/recall/forget → verify hybrid search ranking
- Model router: mock providers → verify fallback chain → verify cost tracking
- Orchestrator: mock agents → verify delegation → verify parallel execution
- Scheduler: verify cron parsing → verify schedule creation → verify heartbeat tick
- Retention engine: create records with retention policies → advance time → verify disposition
- Auth: JWT generation/validation → Argon2 hashing → permission checks
- Audit logger: verify every event type is logged correctly → verify immutability

**TypeScript API (integration):**
- Every REST endpoint: auth required → correct response shape → proper error codes
- WebSocket: connection → auth → message streaming → reconnection
- Rate limiting: verify enforcement at configured thresholds

**Frontend (unit + E2E):**
- Component rendering with mock data
- E2E: login → send message → see streaming response → verify conversation appears in list

### 22.3 LLM Testing

LLM responses are non-deterministic. Testing strategy:

```
1. Mock responses for unit/integration tests:
   - Pre-recorded response fixtures for each test scenario
   - Deterministic: same input always produces same test output

2. Live integration tests (run manually, not in CI):
   - LIVE_TEST=1 cargo test --features live
   - Uses real API keys, real providers
   - Verifies provider adapters work with actual APIs
   - Budget-capped: max $1.00 per test run

3. Evaluation suite (periodic, not per-commit):
   - Standard prompts → measure response quality
   - Tool calling accuracy: does the agent call the right tools?
   - Memory extraction accuracy: does auto_extract capture the right facts?
   - Skill creation quality: are generated skills actually reusable?
```

### 22.4 CI Pipeline

```yaml
# On every PR:
- cargo check                   # type checking
- cargo clippy                  # linting
- cargo test                    # unit + integration (testcontainers)
- npm run test --workspace=api  # API tests
- npm run test --workspace=frontend  # component tests
- cargo audit                   # dependency vulnerability check
- npm audit                     # JS dependency check

# On merge to main:
- All of the above
- E2E tests (Playwright)
- Docker build verification
- Security scan (trivy)

# Weekly:
- Live integration tests (real providers)
- Evaluation suite
- Dependency update check
```

---

## 23. Versioning & Upgrade Path

### 23.1 Semantic Versioning

The platform follows strict semver: `MAJOR.MINOR.PATCH`

- MAJOR: breaking changes to config format, API, or database schema
- MINOR: new features, backward-compatible
- PATCH: bug fixes only

### 23.2 Config File Versioning

```yaml
# Every config.yaml includes a version field
config_version: 1

platform:
  name: "My Agent Platform"
  # ...
```

When the config schema changes in a new release:
- The platform detects `config_version` mismatch on startup
- Runs an automatic migration: reads old config, transforms to new format, writes backup + new file
- Logs every change made during migration
- Refuses to start if migration fails (never silently drops config values)

### 23.3 Database Migration Strategy

- All schema changes are versioned migrations in `migrations/`
- Migrations run automatically on startup (with dry-run option)
- Every migration is reversible (up + down)
- Major version upgrades include a pre-check: "This migration will take ~X minutes on your dataset"
- Backup reminder before major migrations

### 23.4 Skill Format Versioning

```yaml
# skills/my-skill/SKILL.yaml
skill:
  format_version: 1           # skill manifest format version
  name: "web-research"
  version: "1.0.0"            # skill content version
  # ...
```

When the skill format changes, the platform can load both old and new format skills. Old format skills are auto-migrated on first load.

---

## 24. Licensing & Contribution Strategy

### 24.1 License Choice: AGPL-3.0 with Dual Licensing

**Core platform: AGPL-3.0 (GNU Affero General Public License v3)**

AGPL is a real, OSI-approved open-source license. It's the same license used by MongoDB, Grafana, MinIO, Mattermost, and Nextcloud — all of which built large businesses and contributor communities around it. AGPL ensures:

- Anyone can use, modify, self-host, and distribute the platform freely
- If someone offers the platform as a hosted service, they must open-source their modifications — this prevents cloud providers from taking the code and undercutting you
- Contributors know their work stays open and can't be captured by a proprietary fork
- The community is protected: no one can close what's been opened

**Why AGPL over MIT:**

Open WebUI started BSD-3 and had to retroactively add branding restrictions when companies stripped their name and resold it. The community backlash was about the mid-stream change, not the restriction itself. Starting AGPL from day one means there's nothing to change later and no trust to break.

MIT is great for libraries and tools. For a full product that's also a business, AGPL provides the sustainability guarantee that makes the project worth contributing to long-term.

**Why AGPL over BSL:**

BSL is not OSI-approved open source. Some developers won't contribute to or use BSL projects on principle. AGPL is universally recognized as open source, which matters for community building and for attracting co-builders.

**Dual licensing for commercial use:**

Organizations that don't want AGPL obligations (e.g., they want to embed the platform in a proprietary product, or they want to offer it as a service without open-sourcing their modifications) can purchase a commercial license.

```
┌──────────────────────────────────────────────────────┐
│                    AGPL-3.0                           │
│                                                       │
│  ✓ Self-host for internal use (any org size)         │
│  ✓ Modify and extend                                 │
│  ✓ Distribute (with source code)                     │
│  ✓ Use commercially (internal use)                   │
│  ✗ Offer as SaaS without open-sourcing modifications │
│  ✗ Embed in proprietary products                     │
│                                                       │
├──────────────────────────────────────────────────────┤
│               Commercial License                      │
│                                                       │
│  ✓ Everything in AGPL                                │
│  ✓ Offer as SaaS without open-sourcing               │
│  ✓ Embed in proprietary products                     │
│  ✓ White-label / remove branding                     │
│  ✓ Enterprise features (SSO, legal holds, etc.)      │
│  ✓ Priority support and SLA                          │
│                                                       │
└──────────────────────────────────────────────────────┘
```

**CLA (Contributor License Agreement):**

Required for all contributions. This preserves the right to offer commercial licenses alongside the AGPL core. Without a CLA, every contributor would need to individually agree to dual licensing, which becomes unmanageable at scale.

Use the Apache ICLA (Individual Contributor License Agreement) as the template. Automate via CLA Assistant bot on GitHub — contributors sign once via GitHub OAuth.

**Important: The CLA does NOT transfer copyright.** Contributors retain copyright of their contributions. The CLA grants the project a perpetual, non-exclusive license to use the contribution under both AGPL and the commercial license. This is the same model used by GitLab, Grafana, and MongoDB.

### 24.2 What's AGPL vs. Commercial-Only

| Component | License |
|---|---|
| Agent runtime, ReAct loop, tool system | AGPL-3.0 |
| Memory system (all backends, pluggable) | AGPL-3.0 |
| Multi-agent orchestration | AGPL-3.0 |
| Scheduling & heartbeat | AGPL-3.0 |
| Document workspace (local + connectors) | AGPL-3.0 |
| Web UI (all core views) | AGPL-3.0 |
| REST API & WebSocket | AGPL-3.0 |
| Basic auth (JWT + Argon2) | AGPL-3.0 |
| Basic retention (auto-delete policies) | AGPL-3.0 |
| Audit logging | AGPL-3.0 |
| Self-improving skills | AGPL-3.0 |
| Sandbox / security layer | AGPL-3.0 |
| Observability (metrics, tracing) | AGPL-3.0 |
| Multi-user + roles | AGPL-3.0 |
| --- | --- |
| SSO / SAML / OIDC integration | Commercial License (Team+) |
| Legal hold system | Commercial License (Team+) |
| Advanced retention workflows (review gates) | Commercial License (Team+) |
| GDPR compliance toolkit (erasure, portability) | Commercial License (Enterprise) |
| Compliance reporting templates | Commercial License (Enterprise) |
| White-labeling / custom branding | Commercial License (Team+) |
| Priority support and SLA | Commercial License (Pro+) |
| Hosted/managed offering | Commercial License (all paid tiers) |

**Note:** The AGPL core is fully functional. This is not "open core" where the free version is crippled. A solo developer or small team can self-host the full platform with all agents, memory, scheduling, multi-user, and basic retention — for free, forever. The commercial features are enterprise governance and support that only large organizations need.

### 24.3 Attracting Co-Builders

AGPL + CLA + a clear roadmap is the recipe for attracting contributors who might become co-founders or core maintainers.

**What makes people want to contribute:**

- A clear, well-documented architecture (this PRD)
- "Good first issue" labels on GitHub — specific, scoped, achievable
- A CONTRIBUTING.md that explains: how to set up dev environment, how to run tests, how the crate structure works, how to submit a PR
- Fast PR review turnaround (< 48 hours for first response)
- Public roadmap where contributors can claim features
- Recognition: CONTRIBUTORS.md, release notes credit, Discord roles
- The license guarantees their work stays open — this matters to serious contributors

**Contribution paths (not just code):**

| Path | Examples |
|---|---|
| Core development | Rust crate work, new tools, provider adapters |
| Frontend | React components, UI/UX improvements, accessibility |
| Skills | Built-in skills, community skill contributions |
| Documentation | Guides, tutorials, translations |
| Testing | Test coverage, E2E tests, load testing |
| DevOps | Helm charts, Terraform modules, deployment guides |
| Design | UI/UX design, icon sets, design system |
| Community | Discord moderation, issue triage, user support |

**The path from contributor to co-builder:**

```
Contributor (1-5 merged PRs)
  → Regular contributor (consistent activity over 2+ months)
    → Maintainer (commit access to specific crates/areas)
      → Core team (architecture decisions, roadmap input)
        → Co-founder/partner (equity or revenue share, if the business warrants it)
```

### 24.4 Contribution Guidelines

- All contributions require signed CLA (automated via GitHub bot)
- PRs require: passing CI, tests for new functionality, documentation updates, changelog entry
- Security issues: responsible disclosure via security@yourdomain.com (no public issues)
- Feature proposals: RFC process via GitHub Discussions — write a short proposal, community discusses, maintainer decides
- Code style: `cargo fmt` + `cargo clippy` for Rust, Prettier + ESLint for TypeScript — enforced in CI
- Commit messages: Conventional Commits format (`feat:`, `fix:`, `docs:`, `refactor:`, etc.)

---

## 25. Documentation Strategy

Documentation is the sales team for a solo founder. It's not a Phase 7 afterthought — it starts in Phase 1 and grows with every feature.

### 25.1 Documentation Types

| Type | Audience | Format | When |
|---|---|---|---|
| Quick Start | New users | Step-by-step guide, copy-paste ready | Phase 1 |
| Configuration Reference | Admins | Complete YAML reference with every option | Phase 1, updated each phase |
| API Reference | Developers | Auto-generated from OpenAPI spec | Phase 1 |
| Agent Definition Guide | Power users | How to create and configure agents | Phase 2 |
| Memory System Guide | Power users | How memory works, tuning, external backends | Phase 2 |
| Scheduling Guide | Users | How to create schedules, triggers, examples | Phase 3 |
| Admin Guide | Admins | User management, retention, security, backup | Phase 4-5 |
| Architecture Overview | Contributors | System design, crate structure, how to contribute | Phase 1 |
| Deployment Guide | Ops | Docker Compose, Kubernetes, cloud-specific guides | Phase 1 |
| Video Walkthroughs | All | 3-5 minute videos: setup, first agent, first schedule | Phase 7 |
| Troubleshooting FAQ | All | Common issues and solutions | Ongoing |

### 25.2 Documentation Infrastructure

- Docs site: VitePress or Starlight (Astro) — fast, markdown-based, search built in
- Hosted at `docs.yourdomain.com`
- API reference auto-generated from OpenAPI spec
- Code examples in every section are tested (extracted and run in CI)
- Versioned docs matching platform versions

### 25.3 Interactive Setup Wizard

For self-hosted users, the first-run experience matters enormously:

```
$ curl -fsSL https://yourdomain.com/install.sh | bash

Welcome to [Platform Name] Setup!

1. Checking prerequisites...
   ✓ Docker 24.0.7
   ✓ Docker Compose 2.24.0
   ✓ 8GB RAM available

2. Choose your LLM provider:
   [1] OpenRouter (200+ models, one API key)
   [2] Anthropic (Claude)
   [3] OpenAI (GPT)
   [4] Ollama (local, no API key needed)
   [5] Configure later

3. Enter your API key: sk-...

4. Starting services...
   ✓ PostgreSQL
   ✓ Redis
   ✓ MinIO
   ✓ Core
   ✓ API
   ✓ Frontend

🚀 Your agent platform is running at http://localhost:3000
   Create your admin account to get started.
```

---

## 26. Competitive Positioning & Moat

### 26.1 Positioning Statement

> [Platform Name] is the open-source agent platform built for teams who need governance. Multi-agent orchestration, persistent memory, document workspace, and compliance-grade retention — self-hosted or cloud, any LLM provider.

**NOT:** "another agent framework" or "OpenClaw alternative"
**IS:** "the agent platform for organizations that take data seriously"

### 26.2 Defensible Differentiators

| Differentiator | Why It's a Moat |
|---|---|
| Retention & compliance built into the foundation | Nobody else has this. Retrofitting retention is extremely hard. |
| Multi-user with per-user memory and cost tracking | OpenClaw and Hermes are single-user. Multi-user is a complete rearchitecture for them. |
| Web-first UI with document workspace | OpenClaw is messaging-first. Hermes is CLI-first. Neither has a document workspace. |
| Pluggable memory backends | Lock-in to a single memory system is a dealbreaker for enterprises with existing knowledge infrastructure. |
| Rust core | Performance moat: faster, safer, smaller deployment footprint than Python/Node competitors. |
| Self-improving skills | Borrowed from Hermes but integrated with the full platform (retention-aware, multi-user, audited). |

### 26.3 Target Markets (Prioritized)

1. **Technical teams at mid-size companies (50-500 employees)** — need agents for internal workflows, can self-host, care about data control. Price-sensitive enough to avoid enterprise platforms, technical enough to configure YAML.

2. **Government agencies and MPOs** — compliance requirements disqualify OpenClaw and Hermes. Retention schedules, audit logs, and legal holds are requirements, not nice-to-haves. Slow procurement but extremely sticky customers.

3. **Consulting firms and agencies** — manage multiple clients, need per-workspace isolation, document-heavy workflows, need to attribute costs per client.

4. **Individual power users** — the open-source single-user tier. Builds community, generates GitHub stars, feeds the funnel to paid tiers.

### 26.4 Go-To-Market (Solo Founder)

```
Phase 1-3 (Months 1-3): Build in public
  - Ship open-source MVP
  - Post build logs on Twitter/X, dev.to, Hacker News
  - GitHub README is the landing page
  - Dogfood daily — use it as your own personal agent

Phase 4-5 (Months 4-5): Community
  - Discord community
  - First external contributors
  - Write "How I built X with [Platform Name]" tutorials
  - Respond to every GitHub issue within 24 hours

Phase 6-7 (Months 6-7): Revenue
  - Launch hosted version (Pro tier)
  - ProductHunt launch
  - Direct outreach to 10 target organizations
  - First paying customers

Months 8-12: Scale
  - Team tier launch
  - Case studies from early customers
  - Conference talks (local meetups → regional conferences)
  - SEO: "agent platform for teams", "self-hosted AI agent", "AI agent compliance"
```

---

## 27. Open Questions

1. **Project name** — needed before Phase 7, not critical for MVP
2. **Skill format** — agentskills.io standard for portability (recommended)
3. **Rust ↔ TypeScript communication** — gRPC vs HTTP vs FFI (recommend starting with HTTP, optimize to gRPC later)
4. **Hosted platform** — Railway, Fly.io, self-managed VPS, or cloud provider
5. **Mobile** — PWA first, native later
6. **Marketplace** — GitHub-based distribution initially
7. **Sandbox backend** — Wasmtime vs Docker for default code execution sandbox
8. **Tracing backend** — Jaeger vs Grafana Tempo for default self-hosted tracing
