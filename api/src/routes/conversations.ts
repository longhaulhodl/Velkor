import { Hono } from "hono";
import * as core from "../lib/core-client.js";

const app = new Hono();

// GET /api/v1/conversations
app.get("/", async (c) => {
  const user = c.get("user");
  const limit = Number(c.req.query("limit") ?? 50);
  const offset = Number(c.req.query("offset") ?? 0);

  const conversations = await core.listConversations(user.id, limit, offset);
  return c.json({ conversations });
});

// GET /api/v1/conversations/:id
app.get("/:id", async (c) => {
  const id = c.req.param("id");
  const conversation = await core.getConversation(id);
  return c.json({ conversation });
});

// DELETE /api/v1/conversations/:id
app.delete("/:id", async (c) => {
  const id = c.req.param("id");
  await core.deleteConversation(id);
  return c.json({ deleted: true });
});

export { app as conversationRoutes };
