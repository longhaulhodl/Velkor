import { create } from 'zustand';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
}

export interface ToolStatus {
  tool: string;
  status: 'started' | 'completed' | 'failed';
  result?: string;
}

interface ChatState {
  conversationId: string | null;
  messages: ChatMessage[];
  streamingText: string;
  isStreaming: boolean;
  activeTools: ToolStatus[];
  error: string | null;

  setConversationId: (id: string | null) => void;
  addUserMessage: (content: string) => void;
  appendStreamText: (text: string) => void;
  finalizeAssistantMessage: () => void;
  setToolStatus: (status: ToolStatus) => void;
  clearActiveTools: () => void;
  setStreaming: (v: boolean) => void;
  setError: (e: string | null) => void;
  loadMessages: (msgs: ChatMessage[]) => void;
  reset: () => void;
}

let msgCounter = 0;

export const useChatStore = create<ChatState>((set, get) => ({
  conversationId: null,
  messages: [],
  streamingText: '',
  isStreaming: false,
  activeTools: [],
  error: null,

  setConversationId: (id) => set({ conversationId: id }),

  addUserMessage: (content) => {
    const msg: ChatMessage = {
      id: `msg-${++msgCounter}`,
      role: 'user',
      content,
      timestamp: Date.now(),
    };
    set((s) => ({ messages: [...s.messages, msg] }));
  },

  appendStreamText: (text) =>
    set((s) => ({ streamingText: s.streamingText + text })),

  finalizeAssistantMessage: () => {
    const { streamingText } = get();
    if (!streamingText) return;
    const msg: ChatMessage = {
      id: `msg-${++msgCounter}`,
      role: 'assistant',
      content: streamingText,
      timestamp: Date.now(),
    };
    set((s) => ({
      messages: [...s.messages, msg],
      streamingText: '',
      isStreaming: false,
    }));
  },

  setToolStatus: (status) =>
    set((s) => {
      const existing = s.activeTools.findIndex((t) => t.tool === status.tool);
      const tools = [...s.activeTools];
      if (existing >= 0) {
        tools[existing] = status;
      } else {
        tools.push(status);
      }
      return { activeTools: tools };
    }),

  clearActiveTools: () => set({ activeTools: [] }),
  setStreaming: (v) => set({ isStreaming: v }),
  setError: (e) => set({ error: e }),

  loadMessages: (msgs) => set({ messages: msgs }),

  reset: () =>
    set({
      conversationId: null,
      messages: [],
      streamingText: '',
      isStreaming: false,
      activeTools: [],
      error: null,
    }),
}));
