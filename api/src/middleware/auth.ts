import { createMiddleware } from "hono/factory";
import jwt from "jsonwebtoken";
import type { AuthUser } from "../lib/types.js";

const JWT_SECRET = process.env.JWT_SECRET ?? "velkor-dev-secret-change-me";

/** Hono context variable key for the authenticated user. */
declare module "hono" {
  interface ContextVariableMap {
    user: AuthUser;
  }
}

/**
 * JWT authentication middleware.
 *
 * Expects: `Authorization: Bearer <token>`
 * Sets `c.get("user")` on success.
 */
export const authMiddleware = createMiddleware(async (c, next) => {
  const header = c.req.header("Authorization");
  if (!header?.startsWith("Bearer ")) {
    return c.json({ error: "Missing or invalid Authorization header" }, 401);
  }

  const token = header.slice(7);

  try {
    const payload = jwt.verify(token, JWT_SECRET) as AuthUser & { iat: number; exp: number };
    c.set("user", {
      id: payload.id,
      email: payload.email,
      role: payload.role,
      org_id: payload.org_id,
    });
  } catch {
    return c.json({ error: "Invalid or expired token" }, 401);
  }

  await next();
});

/**
 * Require a specific role (or higher).
 * Role hierarchy: admin > member > viewer
 */
export function requireRole(minRole: "admin" | "member" | "viewer") {
  const hierarchy = { viewer: 0, member: 1, admin: 2 };

  return createMiddleware(async (c, next) => {
    const user = c.get("user");
    if (!user) {
      return c.json({ error: "Not authenticated" }, 401);
    }
    if (hierarchy[user.role] < hierarchy[minRole]) {
      return c.json({ error: "Insufficient permissions" }, 403);
    }
    await next();
  });
}

/** Sign a JWT for a user. */
export function signToken(user: AuthUser, expiresIn: string = "24h"): string {
  return jwt.sign(
    { id: user.id, email: user.email, role: user.role, org_id: user.org_id },
    JWT_SECRET,
    { expiresIn: expiresIn as jwt.SignOptions["expiresIn"] }
  );
}

/** Verify a JWT and return the payload (for WebSocket auth). */
export function verifyToken(token: string): AuthUser | null {
  try {
    const payload = jwt.verify(token, JWT_SECRET) as AuthUser;
    return { id: payload.id, email: payload.email, role: payload.role, org_id: payload.org_id };
  } catch {
    return null;
  }
}
