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
    request<{ token: string; user: User }>('/auth/login', {
      method: 'POST',
      body: JSON.stringify({ email, password }),
    }),

  register: (email: string, password: string, display_name: string) =>
    request<{ token: string; user: User }>('/auth/register', {
      method: 'POST',
      body: JSON.stringify({ email, password, display_name }),
    }),

  me: async () => {
    const res = await request<{ user: User }>('/auth/me');
    return res.user;
  },

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

export interface MemoryResult {
  id: string;
  content: string;
  scope: string;
  category: string | null;
  confidence: number;
  score: number;
  created_at: string;
}
