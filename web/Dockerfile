# ---------------------------------------------------------------------------
# Velkor Web Frontend — Vite build served by nginx
# ---------------------------------------------------------------------------
FROM node:22-slim AS builder

WORKDIR /build
COPY package.json package-lock.json* ./
RUN npm ci

COPY . .
RUN npx vite build

# ---------------------------------------------------------------------------
# Serve with nginx
# ---------------------------------------------------------------------------
FROM nginx:alpine

COPY --from=builder /build/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
