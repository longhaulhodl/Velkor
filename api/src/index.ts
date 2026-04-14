import { Hono } from "hono";
import { cors } from "hono/cors";
import { logger } from "hono/logger";
import { serve } from "@hono/node-server";
import { authMiddleware } from "./middleware/auth.js";
import { authRoutes } from "./routes/auth.js";
import { conversationRoutes } from "./routes/conversations.js";
import { memoryRoutes } from "./routes/memory.js";
import { documentRoutes } from "./routes/documents.js";
import { auditRoutes } from "./routes/audit.js";
import { retentionRoutes } from "./routes/retention.js";
import { skillsRoutes } from "./routes/skills.js";
import { wsApp, injectWebSocket } from "./routes/ws.js";
import { CoreError } from "./lib/core-client.js";

const app = new Hono();

// ---------------------------------------------------------------------------
// Global middleware
// ---------------------------------------------------------------------------

app.use("*", logger());
app.use(
  "*",
  cors({
    origin: process.env.CORS_ORIGIN ?? "http://localhost:5173",
    credentials: true,
  })
);

// ---------------------------------------------------------------------------
// Health check (no auth)
// ---------------------------------------------------------------------------

app.get("/health", (c) => c.json({ status: "ok", service: "velkor-api" }));

// ---------------------------------------------------------------------------
// Auth routes (no auth middleware — these issue tokens)
// ---------------------------------------------------------------------------

app.route("/api/v1/auth", authRoutes);

// ---------------------------------------------------------------------------
// WebSocket (auth handled inside the WS handler via query param)
// ---------------------------------------------------------------------------

app.route("/", wsApp);

// ---------------------------------------------------------------------------
// Protected REST routes (all require JWT)
// ---------------------------------------------------------------------------

const api = new Hono();
api.use("*", authMiddleware);
api.route("/conversations", conversationRoutes);
api.route("/memory", memoryRoutes);
api.route("/documents", documentRoutes);
api.route("/audit", auditRoutes);
api.route("/retention", retentionRoutes);
api.route("/skills", skillsRoutes);

app.route("/api/v1", api);

// ---------------------------------------------------------------------------
// Global error handler
// ---------------------------------------------------------------------------

app.onError((err, c) => {
  if (err instanceof CoreError) {
    console.error(`Core API error: ${err.status} ${err.message}`);
    return c.json(
      { error: err.message },
      err.status >= 400 && err.status < 600 ? (err.status as 400) : 502
    );
  }

  console.error("Unhandled error:", err);
  return c.json({ error: "Internal server error" }, 500);
});

// ---------------------------------------------------------------------------
// 404 handler
// ---------------------------------------------------------------------------

app.notFound((c) => c.json({ error: "Not found" }, 404));

// ---------------------------------------------------------------------------
// Start server
// ---------------------------------------------------------------------------

const port = Number(process.env.API_PORT ?? 3000);

const server = serve({ fetch: app.fetch, port }, (info) => {
  console.log(`Velkor API gateway listening on http://localhost:${info.port}`);
});

// Inject WebSocket support into the HTTP server
injectWebSocket(server);

export default app;
