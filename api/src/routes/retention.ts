import { Hono } from "hono";
import { requireRole } from "../middleware/auth.js";
import * as core from "../lib/core-client.js";

const app = new Hono();

// GET /api/v1/retention/status — get retention task status (admin/member only)
app.get("/status", requireRole("member"), async (c) => {
  const status = await core.getRetentionStatus();
  return c.json(status);
});

export { app as retentionRoutes };
