const BASE = '/api/v1';

async function request<T>(path: string, opts: RequestInit = {}): Promise<T> {
  const token = localStorage.getItem('token');
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...((opts.headers as Record<string, string>) ?? {}),
  };
  if (token) headers['Authorization'] = `Bearer ${token}`;

  const res = await fetch(`${BASE}${path}`, { ...opts, headers });
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(text || `${res.status} ${res.statusText}`);
  }
  return res.json();
}

export const api = {
  // Auth
  login: (email: string, password: string) =>
    request<{ token: string; refresh_token: string; user: User }>('/auth/login', {
      method: 'POST',
      body: JSON.stringify({ email, password }),
    }),

  register: (email: string, password: string, display_name: string) =>
    request<{ token: string; refresh_token: string; user: User }>('/auth/register', {
      method: 'POST',
      body: JSON.stringify({ email, password, name: display_name }),
    }),

  me: async () => {
    const res = await request<{ user: User }>('/auth/me');
    return res.user;
  },

  refreshToken: (refreshToken: string) =>
    request<{ token: string }>('/auth/refresh', {
      method: 'POST',
      body: JSON.stringify({ refresh_token: refreshToken }),
    }),

  // Conversations
  listConversations: async (limit = 50, offset = 0) => {
    const res = await request<{ conversations: Conversation[] }>(`/conversations?limit=${limit}&offset=${offset}`);
    return res.conversations;
  },

  getConversation: async (id: string) => {
    const res = await request<{ conversation: ConversationDetail }>(`/conversations/${id}`);
    return res.conversation;
  },

  deleteConversation: (id: string) =>
    request<void>(`/conversations/${id}`, { method: 'DELETE' }),

  // Memory
  searchMemory: (query: string, scope = 'personal', limit = 10) =>
    request<MemoryResult[]>('/memory/search', {
      method: 'POST',
      body: JSON.stringify({ query, scope, limit }),
    }),

  // Documents
  uploadDocument: async (workspaceId: string, file: File) => {
    const token = localStorage.getItem('token');
    const form = new FormData();
    form.append('workspace_id', workspaceId);
    form.append('file', file);

    const res = await fetch(`${BASE}/documents`, {
      method: 'POST',
      headers: token ? { Authorization: `Bearer ${token}` } : {},
      body: form,
    });
    if (!res.ok) throw new Error(await res.text().catch(() => `${res.status}`));
    return res.json() as Promise<{ document: DocumentMeta }>;
  },

  listDocuments: (workspaceId: string) =>
    request<{ documents: DocumentMeta[] }>(`/documents?workspace_id=${workspaceId}`),

  deleteDocument: (id: string) =>
    request<void>(`/documents/${id}`, { method: 'DELETE' }),

  // Pulse (unified background engine)
  getPulseStatus: () =>
    request<PulseStatus>('/pulse/status'),

  // Retention
  getRetentionStatus: () =>
    request<RetentionStatus>('/retention/status'),

  // Skills
  listSkills: () =>
    request<{ skills: SkillSummary[] }>('/skills'),

  listLearnedSkills: () =>
    request<{ skills: LearnedSkill[] }>('/skills/learned'),

  listInstallableSkills: () =>
    request<{ skills: SkillSummary[] }>('/skills/installable'),

  viewSkill: (name: string) =>
    request<SkillDetail>(`/skills/${encodeURIComponent(name)}/view`),

  createLearnedSkill: (body: { name: string; description?: string; content: string; category?: string }) =>
    request<{ id: string; name: string; version: number }>('/skills/learned', {
      method: 'POST',
      body: JSON.stringify(body),
    }),

  patchLearnedSkill: (name: string, body: { content: string; description?: string }) =>
    request<{ name: string; version: number }>(`/skills/learned/${encodeURIComponent(name)}`, {
      method: 'PUT',
      body: JSON.stringify(body),
    }),

  deactivateLearnedSkill: (name: string) =>
    request<{ deactivated: string }>(`/skills/learned/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    }),

  createInstallableSkill: (body: { name: string; description: string; content: string; version?: string; author?: string }) =>
    request<{ name: string; path: string }>('/skills/installable', {
      method: 'POST',
      body: JSON.stringify(body),
    }),

  deleteInstallableSkill: (name: string) =>
    request<{ deleted: string }>(`/skills/installable/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    }),

  reloadInstallableSkills: () =>
    request<{ reloaded: number }>('/skills/reload', { method: 'POST' }),

  // Schedules
  listSchedules: () =>
    request<ScheduleInfo[]>('/schedules'),

  getSchedule: (id: string) =>
    request<ScheduleInfo>(`/schedules/${id}`),

  createSchedule: (body: {
    name: string;
    cron_expression: string;
    task_prompt: string;
    agent_id?: string;
    description?: string;
    natural_language?: string;
    delivery_channel?: string;
    delivery_target?: string;
  }) =>
    request<ScheduleInfo>('/schedules', {
      method: 'POST',
      body: JSON.stringify(body),
    }),

  updateSchedule: (id: string, body: Record<string, unknown>) =>
    request<ScheduleInfo>(`/schedules/${id}`, {
      method: 'PUT',
      body: JSON.stringify(body),
    }),

  deleteSchedule: (id: string) =>
    request<void>(`/schedules/${id}`, { method: 'DELETE' }),

  listScheduleRuns: (scheduleId: string, limit = 50) =>
    request<ScheduleRunInfo[]>(`/schedules/${scheduleId}/runs?limit=${limit}`),

  getSchedulerStatus: () =>
    request<SchedulerStatus>('/schedules/status'),

  // Tasks
  listTasks: (limit = 50) =>
    request<BackgroundTaskInfo[]>(`/tasks?limit=${limit}`),

  getTask: (id: string) =>
    request<BackgroundTaskInfo>(`/tasks/${id}`),

  spawnTask: (body: {
    title: string;
    task_prompt: string;
    agent_id?: string;
    source_conversation_id?: string;
  }) =>
    request<{ task_id: string; status: string }>('/tasks', {
      method: 'POST',
      body: JSON.stringify(body),
    }),

  cancelTask: (id: string) =>
    request<{ cancelled: boolean }>(`/tasks/${id}/cancel`, { method: 'POST' }),

  listAgents: () =>
    request<{ agents: AgentInfo[]; supervisor: string }>('/tasks/agents'),

  // Audit
  searchAudit: async (params: {
    event_type?: string;
    from?: string;
    to?: string;
    limit?: number;
    offset?: number;
  } = {}) => {
    const qs = new URLSearchParams();
    for (const [k, v] of Object.entries(params)) {
      if (v !== undefined) qs.set(k, String(v));
    }
    const res = await request<{ entries: AuditEntry[] }>(`/audit?${qs}`);
    return res.entries;
  },
};

// Types
export interface User {
  id: string;
  email: string;
  display_name: string;
  role: string;
}

export interface Conversation {
  id: string;
  title: string | null;
  summary: string | null;
  started_at: string;
  ended_at: string | null;
}

export interface ConversationDetail extends Conversation {
  messages: Message[];
}

export interface Message {
  role: string;
  content: string;
  created_at: string;
}

export interface DocumentMeta {
  id: string;
  filename: string;
  mime_type: string | null;
  file_size: number | null;
  created_at: string;
}

export interface MemoryResult {
  id: string;
  content: string;
  scope: string;
  category: string | null;
  confidence: number;
  score: number;
  created_at: string;
}

export interface RetentionStatus {
  running: boolean;
  subsystem: SubsystemStatus | null;
}

export interface SkillSummary {
  name: string;
  description: string;
  source: 'installed' | 'learned';
}

export interface LearnedSkill {
  id: string;
  name: string;
  description: string | null;
  category: string | null;
  author: string;
  usage_count: number;
  success_rate: number;
  version: number;
  is_active: boolean;
  created_at: string;
  last_used_at: string | null;
  last_improved_at: string | null;
}

export interface SkillDetail {
  name: string;
  description: string | null;
  version: string | number | null;
  author: string | null;
  source: 'installed' | 'learned';
  content: string;
  source_path?: string;
  category?: string;
  usage_count?: number;
  success_rate?: number;
  created_at?: string;
  last_used_at?: string | null;
  last_improved_at?: string | null;
}

export interface ScheduleInfo {
  id: string;
  user_id: string;
  agent_id: string;
  name: string;
  description: string | null;
  cron_expression: string;
  natural_language: string | null;
  task_prompt: string;
  delivery_channel: string | null;
  delivery_target: string | null;
  is_active: boolean;
  last_run_at: string | null;
  next_run_at: string | null;
  run_count: number;
  error_count: number;
  last_error: string | null;
  created_at: string;
}

export interface ScheduleRunInfo {
  id: string;
  schedule_id: string;
  started_at: string;
  completed_at: string | null;
  status: string | null;
  result_summary: string | null;
  conversation_id: string | null;
  tokens_used: number | null;
  cost_usd: number | null;
  error: string | null;
}

export interface PulseStatus {
  enabled: boolean;
  interval_secs: number;
  running: boolean;
  total_ticks: number;
  last_tick_at: string | null;
  last_tick_duration_ms: number;
  subsystems: SubsystemStatus[];
}

export interface SubsystemStatus {
  name: string;
  enabled: boolean;
  total_runs: number;
  total_processed: number;
  total_failed: number;
  last_run_at: string | null;
  last_result: {
    name: string;
    checked: number;
    processed: number;
    failed: number;
    duration_ms: number;
    details: string | null;
  } | null;
}

export interface SchedulerStatus {
  running: boolean;
  subsystem: SubsystemStatus | null;
}

export interface BackgroundTaskInfo {
  id: string;
  user_id: string;
  agent_id: string;
  title: string;
  task_prompt: string;
  status: string;
  result_summary: string | null;
  conversation_id: string | null;
  source_conversation_id: string | null;
  tokens_used: number | null;
  cost_usd: number | null;
  error: string | null;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
}

export interface AgentInfo {
  id: string;
  model: string;
  is_supervisor: boolean;
}

export interface AuditEntry {
  id: string;
  timestamp: string;
  event_type: string;
  user_id: string | null;
  agent_id: string | null;
  conversation_id: string | null;
  details: Record<string, unknown>;
  model_used: string | null;
  tokens_input: number | null;
  tokens_output: number | null;
  cost_usd: number | null;
  request_id: string | null;
}
