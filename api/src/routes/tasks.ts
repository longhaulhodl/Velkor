import { Hono } from "hono";
import { requireRole } from "../middleware/auth.js";
import * as core from "../lib/core-client.js";

const app = new Hono();

// GET /api/v1/tasks — list tasks (admin sees all, member sees own)
app.get("/", requireRole("member"), async (c) => {
  const user = c.get("user");
  const userId = user.role === "admin" ? undefined : user.id;
  const limit = Number(c.req.query("limit") ?? 50);
  const tasks = await core.listTasks(userId, limit);
  return c.json(tasks);
});

// POST /api/v1/tasks — spawn a new background task
app.post("/", requireRole("member"), async (c) => {
  const user = c.get("user");
  const body = await c.req.json();
  const result = await core.spawnTask({
    ...body,
    user_id: user.id,
  });
  return c.json(result, 202);
});

// GET /api/v1/tasks/agents — list available agents
app.get("/agents", requireRole("member"), async (c) => {
  const agents = await core.listAgents();
  return c.json(agents);
});

// GET /api/v1/tasks/:id — get a single task
app.get("/:id", requireRole("member"), async (c) => {
  const id = c.req.param("id");
  const task = await core.getTask(id);
  return c.json(task);
});

// POST /api/v1/tasks/:id/cancel — cancel a task
app.post("/:id/cancel", requireRole("member"), async (c) => {
  const id = c.req.param("id");
  await core.cancelTask(id);
  return c.json({ cancelled: true });
});

export { app as tasksRoutes };
