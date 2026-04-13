import { Hono } from "hono";
import * as core from "../lib/core-client.js";

const app = new Hono();

// GET /api/v1/documents
app.get("/", async (c) => {
  const workspaceId = c.req.query("workspace_id");
  if (!workspaceId) {
    return c.json({ error: "workspace_id query parameter required" }, 400);
  }

  const limit = Number(c.req.query("limit") ?? 50);
  const offset = Number(c.req.query("offset") ?? 0);

  const documents = await core.listDocuments(workspaceId, limit, offset);
  return c.json({ documents });
});

// POST /api/v1/documents — upload
app.post("/", async (c) => {
  const user = c.get("user");
  const formData = await c.req.formData();

  const workspaceId = formData.get("workspace_id") as string;
  const file = formData.get("file") as File | null;

  if (!workspaceId || !file) {
    return c.json({ error: "workspace_id and file are required" }, 400);
  }

  const doc = await core.uploadDocument(workspaceId, user.id, file);
  return c.json({ document: doc }, 201);
});

// GET /api/v1/documents/:id
app.get("/:id", async (c) => {
  const id = c.req.param("id");
  const doc = await core.getDocument(id);
  return c.json({ document: doc });
});

// GET /api/v1/documents/:id/download
app.get("/:id/download", async (c) => {
  const id = c.req.param("id");
  const resp = await core.downloadDocument(id);

  // Stream the file back
  return new Response(resp.body, {
    headers: {
      "Content-Type": resp.headers.get("Content-Type") ?? "application/octet-stream",
      "Content-Disposition": resp.headers.get("Content-Disposition") ?? `attachment; filename="${id}"`,
    },
  });
});

// DELETE /api/v1/documents/:id
app.delete("/:id", async (c) => {
  const id = c.req.param("id");
  await core.deleteDocument(id);
  return c.json({ deleted: true });
});

export { app as documentRoutes };
