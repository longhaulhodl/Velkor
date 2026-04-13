# ---------------------------------------------------------------------------
# Velkor API Gateway — Node.js / Hono
# ---------------------------------------------------------------------------
FROM node:22-slim AS builder

WORKDIR /build
COPY package.json package-lock.json* ./
RUN npm ci

COPY tsconfig.json ./
COPY src/ src/
RUN npx tsc

# ---------------------------------------------------------------------------
# Runtime
# ---------------------------------------------------------------------------
FROM node:22-slim

RUN apt-get update && apt-get install -y --no-install-recommends curl && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY package.json package-lock.json* ./
RUN npm ci --omit=dev

COPY --from=builder /build/dist/ dist/

EXPOSE 3000

CMD ["node", "dist/index.js"]
