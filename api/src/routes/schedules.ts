import { Hono } from "hono";
import { requireRole } from "../middleware/auth.js";
import * as core from "../lib/core-client.js";

const app = new Hono();

// GET /api/v1/schedules — list schedules (admin sees all, member sees own)
app.get("/", requireRole("member"), async (c) => {
  const user = c.get("user");
  const userId = user.role === "admin" ? undefined : user.id;
  const schedules = await core.listSchedules(userId);
  return c.json(schedules);
});

// POST /api/v1/schedules — create a new schedule
app.post("/", requireRole("member"), async (c) => {
  const user = c.get("user");
  const body = await c.req.json();
  const schedule = await core.createSchedule({
    ...body,
    user_id: user.id,
  });
  return c.json(schedule, 201);
});

// GET /api/v1/schedules/status — scheduler health status (admin only)
app.get("/status", requireRole("admin"), async (c) => {
  const status = await core.getSchedulerStatus();
  return c.json(status);
});

// GET /api/v1/schedules/:id — get a single schedule
app.get("/:id", requireRole("member"), async (c) => {
  const id = c.req.param("id");
  const schedule = await core.getSchedule(id);
  return c.json(schedule);
});

// PUT /api/v1/schedules/:id — update a schedule
app.put("/:id", requireRole("member"), async (c) => {
  const id = c.req.param("id");
  const body = await c.req.json();
  const schedule = await core.updateSchedule(id, body);
  return c.json(schedule);
});

// DELETE /api/v1/schedules/:id — delete a schedule
app.delete("/:id", requireRole("member"), async (c) => {
  const id = c.req.param("id");
  await core.deleteSchedule(id);
  return c.body(null, 204);
});

// GET /api/v1/schedules/:id/runs — get run history for a schedule
app.get("/:id/runs", requireRole("member"), async (c) => {
  const id = c.req.param("id");
  const limit = Number(c.req.query("limit") ?? 50);
  const runs = await core.listScheduleRuns(id, limit);
  return c.json(runs);
});

export { app as schedulesRoutes };
