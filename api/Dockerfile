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

WORKDIR /app

COPY package.json package-lock.json* ./
RUN npm ci --omit=dev

COPY --from=builder /build/dist/ dist/

EXPOSE 3000

CMD ["node", "dist/index.js"]
