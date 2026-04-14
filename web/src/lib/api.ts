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
