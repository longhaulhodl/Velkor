# ---------------------------------------------------------------------------
# Velkor Core — multi-stage Rust build
# ---------------------------------------------------------------------------
FROM rust:1.87-bookworm AS builder

WORKDIR /build

# Cache dependency build: copy manifests first
COPY Cargo.toml Cargo.lock ./
COPY crates/config/Cargo.toml   crates/config/Cargo.toml
COPY crates/db/Cargo.toml       crates/db/Cargo.toml
COPY crates/audit/Cargo.toml    crates/audit/Cargo.toml
COPY crates/memory/Cargo.toml   crates/memory/Cargo.toml
COPY crates/models/Cargo.toml   crates/models/Cargo.toml
COPY crates/runtime/Cargo.toml  crates/runtime/Cargo.toml
COPY crates/orchestrator/Cargo.toml crates/orchestrator/Cargo.toml
COPY crates/scheduler/Cargo.toml    crates/scheduler/Cargo.toml
COPY crates/skills/Cargo.toml       crates/skills/Cargo.toml
COPY crates/documents/Cargo.toml    crates/documents/Cargo.toml
COPY crates/retention/Cargo.toml    crates/retention/Cargo.toml
COPY crates/auth/Cargo.toml         crates/auth/Cargo.toml
COPY crates/tools/Cargo.toml        crates/tools/Cargo.toml
COPY crates/security/Cargo.toml     crates/security/Cargo.toml
COPY crates/observability/Cargo.toml crates/observability/Cargo.toml

# Create dummy lib.rs files so cargo can resolve the workspace
RUN for d in crates/*/; do mkdir -p "$d/src" && echo "" > "$d/src/lib.rs"; done && \
    mkdir -p src && echo "fn main() {}" > src/main.rs

RUN cargo build --release 2>/dev/null || true

# Now copy real source and build
COPY . .
# Touch all source files so cargo knows they changed
RUN find . -name "*.rs" -exec touch {} +
RUN cargo build --release

# ---------------------------------------------------------------------------
# Runtime image
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/velkor /usr/local/bin/velkor

WORKDIR /app
EXPOSE 3001

CMD ["velkor"]
