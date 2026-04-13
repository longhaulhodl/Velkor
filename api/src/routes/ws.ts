import { Hono } from "hono";
import type { WSContext } from "hono/ws";
import { createNodeWebSocket } from "@hono/node-ws";
import { verifyToken } from "../middleware/auth.js";
import { chatStream } from "../lib/core-client.js";
import type { AuthUser, WsClientMessage, WsServerMessage } from "../lib/types.js";

const app = new Hono();

// Node.js WebSocket adapter for Hono
const { injectWebSocket, upgradeWebSocket } = createNodeWebSocket({ app });

/**
 * WebSocket chat endpoint.
 *
 * Connection flow:
 * 1. Client connects to /ws?token=<jwt>
 * 2. Server validates JWT from query param (WebSocket can't use headers easily)
 * 3. Client sends { type: "chat", content: "...", agent_id?: "...", conversation_id?: "..." }
 * 4. Server streams back: text chunks, tool status, done, or error
 */
app.get(
  "/ws",
  upgradeWebSocket((c) => {
    let user: AuthUser | null = null;

    return {
      onOpen(_event: Event, ws: WSContext) {
        // Authenticate from query param
        const token = new URL(c.req.url).searchParams.get("token");
        if (!token) {
          sendJson(ws, { type: "error", message: "Missing token query parameter" });
          ws.close(4001, "Missing token");
          return;
        }

        user = verifyToken(token);
        if (!user) {
          sendJson(ws, { type: "error", message: "Invalid or expired token" });
          ws.close(4001, "Invalid token");
          return;
        }
      },

      async onMessage(event: MessageEvent, ws: WSContext) {
        if (!user) {
          sendJson(ws, { type: "error", message: "Not authenticated" });
          return;
        }

        let msg: WsClientMessage;
        try {
          msg = JSON.parse(typeof event.data === "string" ? event.data : "");
        } catch {
          sendJson(ws, { type: "error", message: "Invalid JSON" });
          return;
        }

        if (msg.type === "chat") {
          await handleChat(ws, user, msg);
        } else {
          sendJson(ws, { type: "error", message: `Unknown message type: ${(msg as { type: string }).type}` });
        }
      },

      onClose() {
        // Cleanup if needed (e.g., remove from Redis connection registry)
      },

      onError(event: Event) {
        console.error("WebSocket error:", event);
      },
    };
  })
);

/**
 * Handle a chat message by streaming from the Rust core.
 *
 * The Rust core returns SSE events. We parse them and forward as
 * typed WebSocket messages to the client.
 */
async function handleChat(
  ws: WSContext,
  user: AuthUser,
  msg: WsClientMessage
) {
  const conversationId = msg.conversation_id ?? crypto.randomUUID();
  const agentId = msg.agent_id ?? "default";

  // Notify client of the conversation ID (useful for new conversations)
  if (!msg.conversation_id) {
    sendJson(ws, { type: "conversation_created", conversation_id: conversationId });
  }

  try {
    // Call the Rust core streaming endpoint
    const response = await chatStream({
      user_id: user.id,
      agent_id: agentId,
      conversation_id: conversationId,
      message: msg.content,
      stream: true,
    });

    if (!response.ok) {
      const errBody = await response.text().catch(() => "");
      sendJson(ws, {
        type: "error",
        message: `Core error (${response.status}): ${errBody}`,
      });
      return;
    }

    if (!response.body) {
      sendJson(ws, { type: "error", message: "No response body from core" });
      return;
    }

    // Parse SSE stream from Rust core and forward to WebSocket
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      // Process complete SSE events (separated by double newlines)
      const events = buffer.split("\n\n");
      buffer = events.pop() ?? ""; // Keep incomplete event in buffer

      for (const event of events) {
        const parsed = parseSseEvent(event);
        if (!parsed) continue;

        // Map core SSE events to WebSocket messages
        switch (parsed.event) {
          case "text":
            sendJson(ws, { type: "text", content: parsed.data });
            break;

          case "tool_status": {
            const data = JSON.parse(parsed.data);
            sendJson(ws, {
              type: "tool_status",
              tool: data.tool,
              status: data.status,
            });
            break;
          }

          case "done": {
            const data = JSON.parse(parsed.data);
            sendJson(ws, {
              type: "done",
              request_id: data.request_id,
              iterations: data.iterations,
              usage: data.usage,
            });
            break;
          }

          case "error":
            sendJson(ws, { type: "error", message: parsed.data });
            break;
        }
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.error("Chat stream error:", message);
    sendJson(ws, { type: "error", message: `Stream error: ${message}` });
  }
}

/** Parse a single SSE event block. */
function parseSseEvent(raw: string): { event: string; data: string } | null {
  let event = "message";
  let data = "";

  for (const line of raw.split("\n")) {
    if (line.startsWith("event: ")) {
      event = line.slice(7).trim();
    } else if (line.startsWith("data: ")) {
      data += line.slice(6);
    } else if (line.startsWith("data:")) {
      data += line.slice(5);
    }
  }

  if (!data) return null;
  return { event, data };
}

/** Send a typed JSON message over the WebSocket. */
function sendJson(ws: WSContext, msg: WsServerMessage) {
  try {
    ws.send(JSON.stringify(msg));
  } catch {
    // Client may have disconnected
  }
}

export { app as wsApp, injectWebSocket };
