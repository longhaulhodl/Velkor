import { Hono } from "hono";
import { requireRole } from "../middleware/auth.js";
import * as core from "../lib/core-client.js";

const app = new Hono();

// GET /api/v1/audit — search audit logs (admin/member only)
app.get("/", requireRole("member"), async (c) => {
  const params = {
    user_id: c.req.query("user_id"),
    event_type: c.req.query("event_type"),
    conversation_id: c.req.query("conversation_id"),
    from: c.req.query("from"),
    to: c.req.query("to"),
    limit: c.req.query("limit") ? Number(c.req.query("limit")) : 50,
    offset: c.req.query("offset") ? Number(c.req.query("offset")) : 0,
  };

  const entries = await core.searchAudit(params);
  return c.json({ entries });
});

export { app as auditRoutes };
