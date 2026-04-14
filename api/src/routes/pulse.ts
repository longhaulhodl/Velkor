import { Hono } from "hono";
import { requireRole } from "../middleware/auth.js";
import * as core from "../lib/core-client.js";

const app = new Hono();

// GET /api/v1/pulse/status — get unified pulse engine status (admin only)
app.get("/status", requireRole("admin"), async (c) => {
  const status = await core.getPulseStatus();
  return c.json(status);
});

export { app as pulseRoutes };
