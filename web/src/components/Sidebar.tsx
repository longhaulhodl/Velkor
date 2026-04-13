import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { api, type Conversation } from '../lib/api';
import { useChatStore } from '../stores/chat';
import { useAuthStore } from '../stores/auth';

export default function Sidebar() {
  const { conversationId, setConversationId, loadMessages, reset } = useChatStore();
  const logout = useAuthStore((s) => s.logout);
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const { data: conversations } = useQuery({
    queryKey: ['conversations'],
    queryFn: () => api.listConversations(),
    refetchInterval: 10000,
  });

  const handleNew = () => {
    reset();
  };

  const handleSelect = async (conv: Conversation) => {
    reset();
    setConversationId(conv.id);
    try {
      const detail = await api.getConversation(conv.id);
      if (detail?.messages?.length) {
        loadMessages(
          detail.messages.map((m, i) => ({
            id: `loaded-${i}`,
            role: m.role as 'user' | 'assistant',
            content: m.content,
            timestamp: new Date(m.created_at).getTime(),
          })),
        );
      }
    } catch (e) {
      console.error('[Velkor] Failed to load conversation:', e);
    }
  };

  const handleDelete = async (e: React.MouseEvent, convId: string) => {
    e.stopPropagation();
    try {
      await api.deleteConversation(convId);
      queryClient.invalidateQueries({ queryKey: ['conversations'] });
      if (conversationId === convId) {
        reset();
      }
    } catch (err) {
      console.error('[Velkor] Failed to delete conversation:', err);
    }
  };

  return (
    <div className="w-64 bg-zinc-950 border-r border-zinc-800 flex flex-col h-screen">
      <div className="p-3">
        <button
          onClick={handleNew}
          className="w-full py-2 px-3 text-sm text-zinc-300 border border-zinc-700 rounded-lg hover:bg-zinc-900 transition-colors text-left"
        >
          + New conversation
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-2">
        {conversations?.map((conv) => (
          <div
            key={conv.id}
            className={`group flex items-center rounded-lg mb-0.5 transition-colors ${
              conversationId === conv.id
                ? 'bg-zinc-800'
                : 'hover:bg-zinc-900'
            }`}
          >
            <button
              onClick={() => handleSelect(conv)}
              className={`flex-1 text-left px-3 py-2 text-sm truncate ${
                conversationId === conv.id
                  ? 'text-white'
                  : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              {conv.title || 'Untitled'}
            </button>
            <button
              onClick={(e) => handleDelete(e, conv.id)}
              title="Delete conversation"
              className="hidden group-hover:block px-2 py-1 text-zinc-600 hover:text-red-400 transition-colors shrink-0"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
              </svg>
            </button>
          </div>
        ))}
      </div>

      <div className="p-3 border-t border-zinc-800 flex items-center justify-between">
        <button
          onClick={() => navigate('/settings')}
          className="text-zinc-500 text-sm hover:text-zinc-300 transition-colors"
        >
          Settings
        </button>
        <button
          onClick={logout}
          className="text-zinc-500 text-sm hover:text-zinc-300 transition-colors"
        >
          Sign out
        </button>
      </div>
    </div>
  );
}
