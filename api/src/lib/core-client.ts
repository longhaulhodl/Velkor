/**
 * Typed client for the Rust core HTTP API.
 *
 * The Rust core runs as a separate process exposing internal HTTP endpoints.
 * This client wraps those calls with proper types.
 */

const CORE_URL = process.env.CORE_URL ?? "http://localhost:3001";

export interface ChatRequest {
  user_id: string;
  agent_id: string;
  conversation_id: string;
  message: string;
  stream: boolean;
}

export interface MemoryStoreRequest {
  user_id: string;
  content: string;
  scope: "personal" | "shared" | "org";
  category?: "fact" | "preference" | "project" | "procedure" | "relationship";
  source_conversation_id?: string;
}

export interface MemorySearchRequest {
  user_id: string;
  query: string;
  scope: "personal" | "shared" | "org";
  limit: number;
}

export interface MemoryRecord {
  id: string;
  content: string;
  scope: string;
  category?: string;
  confidence: number;
  score?: number;
  created_at: string;
}

export interface Conversation {
  id: string;
  title?: string;
  summary?: string;
  started_at: string;
  ended_at?: string;
  messages?: ConversationMessage[];
}

export interface ConversationMessage {
  role: string;
  content: string;
  created_at: string;
}

export interface DocumentMeta {
  id: string;
  workspace_id: string;
  filename: string;
  mime_type?: string;
  file_size?: number;
  created_at: string;
}

export interface AuditEntry {
  id: string;
  timestamp: string;
  event_type: string;
  user_id?: string;
  agent_id?: string;
  conversation_id?: string;
  details: Record<string, unknown>;
}

async function coreRequest<T>(
  path: string,
  init?: RequestInit
): Promise<T> {
  const url = `${CORE_URL}${path}`;
  const resp = await fetch(url, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...init?.headers,
    },
  });

  if (!resp.ok) {
    const body = await resp.text().catch(() => "");
    throw new CoreError(resp.status, body || resp.statusText);
  }

  return resp.json() as Promise<T>;
}

export class CoreError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(`Core API error (${status}): ${message}`);
  }
}

// ---------------------------------------------------------------------------
// Chat (streaming)
// ---------------------------------------------------------------------------

/**
 * Start a streaming chat. Returns a ReadableStream of SSE events from the
 * Rust core. The gateway forwards these over the WebSocket.
 */
export async function chatStream(req: ChatRequest): Promise<Response> {
  const url = `${CORE_URL}/internal/chat`;
  return fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
}

// ---------------------------------------------------------------------------
// Conversations
// ---------------------------------------------------------------------------

export async function listConversations(
  userId: string,
  limit = 50,
  offset = 0
): Promise<Conversation[]> {
  return coreRequest(
    `/internal/conversations?user_id=${userId}&limit=${limit}&offset=${offset}`
  );
}

export async function getConversation(id: string): Promise<Conversation> {
  return coreRequest(`/internal/conversations/${id}`);
}

export async function deleteConversation(id: string): Promise<void> {
  await coreRequest(`/internal/conversations/${id}`, { method: "DELETE" });
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

export async function searchMemory(
  req: MemorySearchRequest
): Promise<MemoryRecord[]> {
  return coreRequest("/internal/memory/search", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function storeMemory(
  req: MemoryStoreRequest
): Promise<{ id: string }> {
  return coreRequest("/internal/memory", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateMemory(
  id: string,
  content: string
): Promise<void> {
  await coreRequest(`/internal/memory/${id}`, {
    method: "PUT",
    body: JSON.stringify({ content }),
  });
}

export async function deleteMemory(id: string): Promise<void> {
  await coreRequest(`/internal/memory/${id}`, { method: "DELETE" });
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

export async function listDocuments(
  workspaceId: string,
  limit = 50,
  offset = 0
): Promise<DocumentMeta[]> {
  return coreRequest(
    `/internal/documents?workspace_id=${workspaceId}&limit=${limit}&offset=${offset}`
  );
}

export async function getDocument(id: string): Promise<DocumentMeta> {
  return coreRequest(`/internal/documents/${id}`);
}

export async function deleteDocument(id: string): Promise<void> {
  await coreRequest(`/internal/documents/${id}`, { method: "DELETE" });
}

/**
 * Upload a document. Uses multipart/form-data, not JSON.
 */
export async function uploadDocument(
  workspaceId: string,
  userId: string,
  file: File
): Promise<DocumentMeta> {
  const form = new FormData();
  form.append("workspace_id", workspaceId);
  form.append("user_id", userId);
  form.append("file", file);

  const resp = await fetch(`${CORE_URL}/internal/documents`, {
    method: "POST",
    body: form,
  });

  if (!resp.ok) {
    const body = await resp.text().catch(() => "");
    throw new CoreError(resp.status, body);
  }

  return resp.json() as Promise<DocumentMeta>;
}

export async function downloadDocument(
  id: string
): Promise<Response> {
  const resp = await fetch(`${CORE_URL}/internal/documents/${id}/download`);
  if (!resp.ok) {
    throw new CoreError(resp.status, "download failed");
  }
  return resp;
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

export async function searchAudit(params: {
  user_id?: string;
  event_type?: string;
  conversation_id?: string;
  from?: string;
  to?: string;
  limit?: number;
  offset?: number;
}): Promise<AuditEntry[]> {
  const qs = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined) qs.set(k, String(v));
  }
  return coreRequest(`/internal/audit?${qs}`);
}

// ---------------------------------------------------------------------------
// Retention
// ---------------------------------------------------------------------------

export async function getRetentionStatus(): Promise<unknown> {
  return coreRequest("/internal/retention/status");
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

export async function listSkills(): Promise<unknown> {
  return coreRequest("/internal/skills");
}

export async function listLearnedSkills(): Promise<unknown> {
  return coreRequest("/internal/skills/learned");
}

export async function listInstallableSkills(): Promise<unknown> {
  return coreRequest("/internal/skills/installable");
}

export async function viewSkill(name: string): Promise<unknown> {
  return coreRequest(`/internal/skills/${encodeURIComponent(name)}/view`);
}

export async function createLearnedSkill(body: {
  name: string;
  description?: string;
  content: string;
  category?: string;
}): Promise<unknown> {
  return coreRequest("/internal/skills/learned", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function patchLearnedSkill(
  name: string,
  body: { content: string; description?: string }
): Promise<unknown> {
  return coreRequest(`/internal/skills/learned/${encodeURIComponent(name)}`, {
    method: "PUT",
    body: JSON.stringify(body),
  });
}

export async function deactivateLearnedSkill(name: string): Promise<unknown> {
  return coreRequest(`/internal/skills/learned/${encodeURIComponent(name)}`, {
    method: "DELETE",
  });
}

export async function createInstallableSkill(body: {
  name: string;
  description: string;
  content: string;
  version?: string;
  author?: string;
}): Promise<unknown> {
  return coreRequest("/internal/skills/installable", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function deleteInstallableSkill(name: string): Promise<unknown> {
  return coreRequest(`/internal/skills/installable/${encodeURIComponent(name)}`, {
    method: "DELETE",
  });
}

export async function reloadInstallableSkills(): Promise<unknown> {
  return coreRequest("/internal/skills/reload", { method: "POST" });
}

// ---------------------------------------------------------------------------
// Schedules
// ---------------------------------------------------------------------------

export async function listSchedules(userId?: string): Promise<unknown> {
  const params = userId ? `?user_id=${userId}` : "";
  return coreRequest(`/internal/schedules${params}`);
}

export async function getSchedule(id: string): Promise<unknown> {
  return coreRequest(`/internal/schedules/${id}`);
}

export async function createSchedule(body: {
  user_id: string;
  agent_id?: string;
  name: string;
  description?: string;
  cron_expression: string;
  natural_language?: string;
  task_prompt: string;
  delivery_channel?: string;
  delivery_target?: string;
}): Promise<unknown> {
  return coreRequest("/internal/schedules", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function updateSchedule(
  id: string,
  body: Record<string, unknown>
): Promise<unknown> {
  return coreRequest(`/internal/schedules/${id}`, {
    method: "PUT",
    body: JSON.stringify(body),
  });
}

export async function deleteSchedule(id: string): Promise<void> {
  await coreRequest(`/internal/schedules/${id}`, { method: "DELETE" });
}

export async function listScheduleRuns(
  scheduleId: string,
  limit = 50
): Promise<unknown> {
  return coreRequest(`/internal/schedules/${scheduleId}/runs?limit=${limit}`);
}

export async function getSchedulerStatus(): Promise<unknown> {
  return coreRequest("/internal/schedules/status");
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

export async function listTasks(userId?: string, limit = 50): Promise<unknown> {
  const params = new URLSearchParams();
  if (userId) params.set("user_id", userId);
  params.set("limit", String(limit));
  return coreRequest(`/internal/tasks?${params}`);
}

export async function getTask(id: string): Promise<unknown> {
  return coreRequest(`/internal/tasks/${id}`);
}

export async function spawnTask(body: {
  user_id: string;
  agent_id?: string;
  title: string;
  task_prompt: string;
  source_conversation_id?: string;
}): Promise<unknown> {
  return coreRequest("/internal/tasks", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function cancelTask(id: string): Promise<void> {
  await coreRequest(`/internal/tasks/${id}/cancel`, { method: "POST" });
}

export async function listAgents(): Promise<unknown> {
  return coreRequest("/internal/tasks/agents");
}
