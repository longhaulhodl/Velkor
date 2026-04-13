/** Authenticated user payload embedded in the JWT and available in context. */
export interface AuthUser {
  id: string;
  email: string;
  display_name?: string;
  role: "admin" | "member" | "viewer";
  org_id?: string;
}

/** WebSocket message sent from the client. */
export interface WsClientMessage {
  type: "chat";
  agent_id?: string;
  conversation_id?: string;
  content: string;
}

/** WebSocket message sent to the client. */
export type WsServerMessage =
  | { type: "text"; content: string }
  | { type: "tool_status"; tool: string; status: "started" | "completed" | "failed" }
  | { type: "done"; request_id: string; iterations: number; usage: { input_tokens: number; output_tokens: number } }
  | { type: "error"; message: string }
  | { type: "conversation_created"; conversation_id: string };
