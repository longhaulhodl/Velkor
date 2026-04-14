import { Hono } from "hono";
import { requireRole } from "../middleware/auth.js";
import * as core from "../lib/core-client.js";

const app = new Hono();

// GET /api/v1/skills — list all skills (installable + learned)
app.get("/", requireRole("member"), async (c) => {
  const data = await core.listSkills();
  return c.json(data);
});

// GET /api/v1/skills/installable — list installable skills only
app.get("/installable", requireRole("member"), async (c) => {
  const data = await core.listInstallableSkills();
  return c.json(data);
});

// GET /api/v1/skills/learned — list learned skills with metadata
app.get("/learned", requireRole("member"), async (c) => {
  const data = await core.listLearnedSkills();
  return c.json(data);
});

// GET /api/v1/skills/:name/view — view full skill content
app.get("/:name/view", requireRole("member"), async (c) => {
  const name = c.req.param("name");
  const data = await core.viewSkill(name);
  return c.json(data);
});

// POST /api/v1/skills/learned — create a new learned skill
app.post("/learned", requireRole("admin"), async (c) => {
  const body = await c.req.json();
  const data = await core.createLearnedSkill(body);
  return c.json(data, 201);
});

// PUT /api/v1/skills/learned/:name — patch a learned skill
app.put("/learned/:name", requireRole("admin"), async (c) => {
  const name = c.req.param("name");
  const body = await c.req.json();
  const data = await core.patchLearnedSkill(name, body);
  return c.json(data);
});

// DELETE /api/v1/skills/learned/:name — deactivate a learned skill
app.delete("/learned/:name", requireRole("admin"), async (c) => {
  const name = c.req.param("name");
  const data = await core.deactivateLearnedSkill(name);
  return c.json(data);
});

// POST /api/v1/skills/installable — create an installable skill (writes SKILL.md)
app.post("/installable", requireRole("admin"), async (c) => {
  const body = await c.req.json();
  const data = await core.createInstallableSkill(body);
  return c.json(data, 201);
});

// DELETE /api/v1/skills/installable/:name — delete an installable skill
app.delete("/installable/:name", requireRole("admin"), async (c) => {
  const name = c.req.param("name");
  const data = await core.deleteInstallableSkill(name);
  return c.json(data);
});

// POST /api/v1/skills/reload — reload installable skills from disk
app.post("/reload", requireRole("admin"), async (c) => {
  const data = await core.reloadInstallableSkills();
  return c.json(data);
});

export { app as skillsRoutes };
