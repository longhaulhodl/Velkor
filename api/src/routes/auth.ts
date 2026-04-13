import { Hono } from "hono";
import argon2 from "argon2";
import { signToken, authMiddleware } from "../middleware/auth.js";
import type { AuthUser } from "../lib/types.js";

/**
 * Auth routes — login, register, refresh, me.
 *
 * Phase 1: users are stored in Postgres via the Rust core.
 * The TS gateway handles password hashing (Argon2) and JWT signing.
 */
const app = new Hono();

const CORE_URL = process.env.CORE_URL ?? "http://localhost:3001";

// POST /api/v1/auth/login
app.post("/login", async (c) => {
  const body = await c.req.json<{ email: string; password: string }>();

  if (!body.email || !body.password) {
    return c.json({ error: "Email and password required" }, 400);
  }

  // Fetch user from Rust core (includes password hash)
  const resp = await fetch(`${CORE_URL}/internal/users/by-email/${encodeURIComponent(body.email)}`);
  if (!resp.ok) {
    return c.json({ error: "Invalid email or password" }, 401);
  }

  const user = (await resp.json()) as {
    id: string;
    email: string;
    password_hash: string;
    display_name?: string;
    role: string;
    org_id?: string;
  };

  // Verify password with Argon2
  const valid = await argon2.verify(user.password_hash, body.password);
  if (!valid) {
    return c.json({ error: "Invalid email or password" }, 401);
  }

  const displayName = user.display_name ?? user.email;
  const authUser: AuthUser = {
    id: user.id,
    email: user.email,
    display_name: displayName,
    role: user.role as AuthUser["role"],
    org_id: user.org_id,
  };

  const token = signToken(authUser);
  const refreshToken = signToken(authUser, "7d");

  return c.json({
    token,
    refresh_token: refreshToken,
    user: { id: user.id, email: user.email, display_name: displayName, role: user.role },
  });
});

// POST /api/v1/auth/register
app.post("/register", async (c) => {
  const body = await c.req.json<{
    email: string;
    password: string;
    name?: string;
  }>();

  if (!body.email || !body.password) {
    return c.json({ error: "Email and password required" }, 400);
  }

  if (body.password.length < 8) {
    return c.json({ error: "Password must be at least 8 characters" }, 400);
  }

  // Hash password with Argon2
  const passwordHash = await argon2.hash(body.password);

  // Create user via Rust core
  const resp = await fetch(`${CORE_URL}/internal/users`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      email: body.email,
      password_hash: passwordHash,
      name: body.name,
    }),
  });

  if (!resp.ok) {
    const errBody = await resp.text().catch(() => "");
    if (resp.status === 409) {
      return c.json({ error: "Email already registered" }, 409);
    }
    return c.json({ error: `Registration failed: ${errBody}` }, 500 as const);
  }

  const user = (await resp.json()) as { id: string; email: string; display_name?: string; role: string };
  const displayName = user.display_name ?? user.email;

  const authUser: AuthUser = {
    id: user.id,
    email: user.email,
    display_name: displayName,
    role: user.role as AuthUser["role"],
  };

  const token = signToken(authUser);
  const refreshToken = signToken(authUser, "7d");

  return c.json({
    token,
    refresh_token: refreshToken,
    user: { id: user.id, email: user.email, display_name: user.display_name ?? user.email, role: user.role },
  }, 201);
});

// POST /api/v1/auth/refresh
app.post("/refresh", async (c) => {
  const body = await c.req.json<{ refresh_token: string }>();

  if (!body.refresh_token) {
    return c.json({ error: "refresh_token required" }, 400);
  }

  // The refresh token is just a longer-lived JWT with the same payload
  const { verifyToken } = await import("../middleware/auth.js");
  const user = verifyToken(body.refresh_token);
  if (!user) {
    return c.json({ error: "Invalid or expired refresh token" }, 401);
  }

  const token = signToken(user);
  return c.json({ token });
});

// GET /api/v1/auth/me (protected)
app.get("/me", authMiddleware, (c) => {
  const user = c.get("user");
  return c.json({ user });
});

export { app as authRoutes };
