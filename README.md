<div align="center">

```
 ██╗   ██╗███████╗██╗     ██╗  ██╗ ██████╗ ██████╗
 ██║   ██║██╔════╝██║     ██║ ██╔╝██╔═══██╗██╔══██╗
 ██║   ██║█████╗  ██║     █████╔╝ ██║   ██║██████╔╝
 ╚██╗ ██╔╝██╔══╝  ██║     ██╔═██╗ ██║   ██║██╔══██╗
  ╚████╔╝ ███████╗███████╗██║  ██╗╚██████╔╝██║  ██║
   ╚═══╝  ╚══════╝╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝
```

**Self-hosted multi-agent orchestration platform**

[Install](#install) · [Architecture](#architecture) · [Configuration](#configuration) · [Development](#development)

</div>

---

## Install

One command from zero to running. Requires **Node.js 18+** and **Docker**.

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/longhaulhodl/Velkor/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/longhaulhodl/Velkor/main/scripts/install.ps1 | iex
```

### What happens

1. Checks prerequisites (git, Node.js, Docker, Docker Compose)
2. Clones the repository to `~/velkor` (override with `VELKOR_DIR`)
3. Installs and builds the CLI
4. Launches the interactive setup wizard

The wizard walks you through:
- Choosing an LLM provider (OpenRouter, Anthropic, OpenAI, Ollama, or custom)
- Configuring embeddings for semantic search
- Setting up web search (Perplexity, Tavily, Brave, or DuckDuckGo)
- Creating your admin account
- Starting all services with Docker Compose

### Manual install

```bash
git clone https://github.com/longhaulhodl/Velkor.git
cd velkor
cd cli && npm install && npx tsc && cd ..
node cli/dist/index.js setup
```

---

## Architecture

```
┌──────────┐     ┌──────────────┐     ┌──────────────┐
│  Web UI  │────▶│  API Gateway │────▶│  Rust Core   │
│  :8080   │ ws  │  (Hono) :3000│ http│  (Axum) :3001│
└──────────┘     └──────────────┘     └──────┬───────┘
                                             │
                    ┌────────────────────────┬┴────────────┐
                    │                        │             │
              ┌─────▼─────┐          ┌───────▼──┐   ┌─────▼─────┐
              │ PostgreSQL │          │  Redis   │   │   MinIO   │
              │  pgvector  │          │          │   │   (S3)    │
              │   :5432    │          │  :6379   │   │   :9000   │
              └────────────┘          └──────────┘   └───────────┘
```

| Layer | Technology | Purpose |
|-------|-----------|---------|
| **Web UI** | React + TypeScript + Tailwind | Chat interface, conversation management |
| **API Gateway** | Hono (Node.js) | WebSocket chat, JWT auth, REST endpoints |
| **Core** | Rust (Axum) | ReAct agent loop, memory, tools, model routing |
| **Database** | PostgreSQL 17 + pgvector | Data storage, vector search, FTS |
| **Cache** | Redis 7 | Streams, caching |
| **Storage** | MinIO | S3-compatible document storage |

### Rust Core Crates

| Crate | Purpose |
|-------|---------|
| `velkor-config` | YAML config with env var substitution |
| `velkor-db` | PostgreSQL connection pool |
| `velkor-models` | LLM provider abstraction (Anthropic, OpenAI-compat, embeddings) |
| `velkor-memory` | Hybrid FTS + vector search with RRF scoring |
| `velkor-runtime` | ReAct agent loop (streaming + non-streaming) |
| `velkor-tools` | Tool trait + built-in tools (web search, fetch, memory, documents) |
| `velkor-documents` | Upload, text extraction, hybrid search |
| `velkor-audit` | Append-only audit logging |
| `velkor-auth` | JWT + Argon2 authentication |
| `velkor-scheduler` | Cron-based task scheduling |
| `velkor-retention` | Data retention policies + legal holds |
| `velkor-orchestrator` | Multi-agent delegation |
| `velkor-skills` | Procedural memory / learned skills |
| `velkor-security` | Input validation, rate limiting |
| `velkor-observability` | Metrics, tracing |

---

## Configuration

### Reconfigure sections

After initial setup, change individual sections without redoing the wizard:

```bash
node cli/dist/index.js configure --section llm          # Change LLM provider
node cli/dist/index.js configure --section embeddings    # Change embedding provider
node cli/dist/index.js configure --section web-search    # Change search provider
```

Or run `node cli/dist/index.js configure` for an interactive picker.

### LLM Providers

| Provider | Config key | Notes |
|----------|-----------|-------|
| OpenRouter | `OPENROUTER_API_KEY` | 200+ models, recommended |
| Anthropic | `ANTHROPIC_API_KEY` | Claude direct |
| OpenAI | `OPENAI_API_KEY` | GPT-4o, o3, o4 |
| Ollama | `base_url` | Local, no API key |
| Custom | `base_url` + `api_key` | Any OpenAI-compatible endpoint |

### Web Search Providers

| Provider | Config key | Notes |
|----------|-----------|-------|
| Perplexity | `PERPLEXITY_API_KEY` or `OPENROUTER_API_KEY` | AI-synthesized answers with citations |
| Tavily | `TAVILY_API_KEY` | Purpose-built search API |
| Brave | `BRAVE_API_KEY` | Privacy-focused search |
| DuckDuckGo | — | Free, no key needed |

Auto-detection priority: Tavily → Brave → Serper → Perplexity → DuckDuckGo.

### Embedding Providers

| Provider | Model | Dimensions |
|----------|-------|-----------|
| OpenAI | text-embedding-3-small | 1536 |
| Ollama | nomic-embed-text | 768 |

Embeddings are optional — full-text search works without them.

### Files

| File | Purpose |
|------|---------|
| `.env` | Secrets, API keys, passwords (generated by setup) |
| `config.docker.yaml` | Platform configuration (generated by setup) |
| `config.example.yaml` | Annotated reference configuration |
| `docker-compose.yml` | Service orchestration |

---

## Development

### Prerequisites

- Rust 1.87+ (2024 edition)
- Node.js 18+
- Docker + Docker Compose
- PostgreSQL 17 with pgvector (or use Docker)

### Local development

```bash
# Rust core
cd core
cargo check
cargo run

# API gateway
cd api
npm install
npm run dev

# Web frontend
cd web
npm install
npm run dev

# CLI
cd cli
npm install
npx tsx src/index.ts setup
```

### Project structure

```
velkor/
├── core/               Rust workspace
│   ├── src/            Main binary (axum server)
│   ├── crates/         15 workspace crates
│   └── Cargo.toml      Workspace root
├── api/                TypeScript API gateway (Hono)
├── web/                React frontend (Vite)
├── cli/                Setup wizard CLI
├── migrations/         PostgreSQL schema
├── scripts/            Install + init scripts
├── docker-compose.yml
├── config.docker.yaml
└── .env
```

---

## License

AGPL-3.0
