import { create } from 'zustand';
import { api, type User } from '../lib/api';

interface AuthState {
  user: User | null;
  token: string | null;
  loading: boolean;
  error: string | null;

  login: (email: string, password: string) => Promise<void>;
  register: (email: string, password: string, displayName: string) => Promise<void>;
  logout: () => void;
  checkAuth: () => Promise<void>;
}

// Auto-refresh timer reference
let refreshTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleRefresh(set: (s: Partial<AuthState>) => void) {
  if (refreshTimer) clearTimeout(refreshTimer);
  const refreshToken = localStorage.getItem('refresh_token');
  if (!refreshToken) return;

  // Refresh 5 minutes before the 24h token expires (i.e. after 23h55m)
  // But for practical purposes, refresh every 20 minutes to be safe
  refreshTimer = setTimeout(async () => {
    try {
      const { token } = await api.refreshToken(refreshToken);
      localStorage.setItem('token', token);
      set({ token });
      scheduleRefresh(set); // schedule next refresh
    } catch {
      // Refresh failed — user will need to re-login when token expires
      console.warn('[Velkor] Token refresh failed');
    }
  }, 20 * 60 * 1000); // 20 minutes
}

export const useAuthStore = create<AuthState>((set) => ({
  user: null,
  token: localStorage.getItem('token'),
  loading: false,
  error: null,

  login: async (email, password) => {
    set({ loading: true, error: null });
    try {
      const res = await api.login(email, password);
      localStorage.setItem('token', res.token);
      if ('refresh_token' in res) {
        localStorage.setItem('refresh_token', (res as { refresh_token: string }).refresh_token);
      }
      set({ user: res.user, token: res.token, loading: false });
      scheduleRefresh(set);
    } catch (e) {
      set({ loading: false, error: (e as Error).message });
    }
  },

  register: async (email, password, displayName) => {
    set({ loading: true, error: null });
    try {
      const res = await api.register(email, password, displayName);
      localStorage.setItem('token', res.token);
      if ('refresh_token' in res) {
        localStorage.setItem('refresh_token', (res as { refresh_token: string }).refresh_token);
      }
      set({ user: res.user, token: res.token, loading: false });
      scheduleRefresh(set);
    } catch (e) {
      set({ loading: false, error: (e as Error).message });
    }
  },

  logout: () => {
    if (refreshTimer) clearTimeout(refreshTimer);
    localStorage.removeItem('token');
    localStorage.removeItem('refresh_token');
    localStorage.removeItem('conversationId');
    set({ user: null, token: null });
  },

  checkAuth: async () => {
    const token = localStorage.getItem('token');
    if (!token) return;
    try {
      const user = await api.me();
      set({ user, token });
      scheduleRefresh(set);
    } catch {
      localStorage.removeItem('token');
      localStorage.removeItem('refresh_token');
      set({ user: null, token: null });
    }
  },
}));
