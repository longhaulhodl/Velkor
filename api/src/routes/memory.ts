import { Hono } from "hono";
import * as core from "../lib/core-client.js";

const app = new Hono();

// GET /api/v1/memory — search memories
app.get("/", async (c) => {
  const user = c.get("user");
  const query = c.req.query("q") ?? "";
  const scope = (c.req.query("scope") ?? "personal") as "personal" | "shared" | "org";
  const limit = Number(c.req.query("limit") ?? 10);

  if (!query) {
    return c.json({ error: "Query parameter 'q' is required" }, 400);
  }

  const memories = await core.searchMemory({
    user_id: user.id,
    query,
    scope,
    limit,
  });

  return c.json({ memories });
});

// POST /api/v1/memory/search — search memories (frontend uses this)
app.post("/search", async (c) => {
  const user = c.get("user");
  const body = await c.req.json<{
    query: string;
    scope?: string;
    limit?: number;
  }>();

  if (!body.query) {
    return c.json({ error: "query is required" }, 400);
  }

  const memories = await core.searchMemory({
    user_id: user.id,
    query: body.query,
    scope: (body.scope ?? "personal") as "personal" | "shared" | "org",
    limit: body.limit ?? 10,
  });

  return c.json(memories);
});

// POST /api/v1/memory — store memory
app.post("/", async (c) => {
  const user = c.get("user");
  const body = await c.req.json<{
    content: string;
    scope?: string;
    category?: string;
    source_conversation_id?: string;
  }>();

  if (!body.content) {
    return c.json({ error: "content is required" }, 400);
  }

  const result = await core.storeMemory({
    user_id: user.id,
    content: body.content,
    scope: (body.scope ?? "personal") as "personal" | "shared" | "org",
    category: body.category as core.MemoryStoreRequest["category"],
    source_conversation_id: body.source_conversation_id,
  });

  return c.json(result, 201);
});

// PUT /api/v1/memory/:id — update memory
app.put("/:id", async (c) => {
  const id = c.req.param("id");
  const body = await c.req.json<{ content: string }>();

  if (!body.content) {
    return c.json({ error: "content is required" }, 400);
  }

  await core.updateMemory(id, body.content);
  return c.json({ updated: true });
});

// DELETE /api/v1/memory/:id — delete memory
app.delete("/:id", async (c) => {
  const id = c.req.param("id");
  await core.deleteMemory(id);
  return c.json({ deleted: true });
});

export { app as memoryRoutes };
