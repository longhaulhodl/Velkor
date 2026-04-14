import { create } from 'zustand';

export interface TaskNotification {
  task_id: string;
  title: string;
  status: string;
  result_summary?: string;
  conversation_id?: string;
  error?: string;
  tokens_used: number;
  received_at: number;
  dismissed: boolean;
}

interface NotificationState {
  notifications: TaskNotification[];
  addTaskNotification: (n: Omit<TaskNotification, 'received_at' | 'dismissed'>) => void;
  dismiss: (taskId: string) => void;
  dismissAll: () => void;
  unreadCount: () => number;
}

export const useNotificationStore = create<NotificationState>((set, get) => ({
  notifications: [],

  addTaskNotification: (n) =>
    set((state) => ({
      notifications: [
        { ...n, received_at: Date.now(), dismissed: false },
        ...state.notifications,
      ].slice(0, 100), // keep last 100
    })),

  dismiss: (taskId) =>
    set((state) => ({
      notifications: state.notifications.map((n) =>
        n.task_id === taskId ? { ...n, dismissed: true } : n
      ),
    })),

  dismissAll: () =>
    set((state) => ({
      notifications: state.notifications.map((n) => ({ ...n, dismissed: true })),
    })),

  unreadCount: () => get().notifications.filter((n) => !n.dismissed).length,
}));
