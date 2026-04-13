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

export const useAuthStore = create<AuthState>((set) => ({
  user: null,
  token: localStorage.getItem('token'),
  loading: false,
  error: null,

  login: async (email, password) => {
    set({ loading: true, error: null });
    try {
      const { token, user } = await api.login(email, password);
      localStorage.setItem('token', token);
      set({ user, token, loading: false });
    } catch (e) {
      set({ loading: false, error: (e as Error).message });
    }
  },

  register: async (email, password, displayName) => {
    set({ loading: true, error: null });
    try {
      const { token, user } = await api.register(email, password, displayName);
      localStorage.setItem('token', token);
      set({ user, token, loading: false });
    } catch (e) {
      set({ loading: false, error: (e as Error).message });
    }
  },

  logout: () => {
    localStorage.removeItem('token');
    set({ user: null, token: null });
  },

  checkAuth: async () => {
    const token = localStorage.getItem('token');
    if (!token) return;
    try {
      const user = await api.me();
      set({ user, token });
    } catch {
      localStorage.removeItem('token');
      set({ user: null, token: null });
    }
  },
}));
