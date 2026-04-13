import { useQuery } from '@tanstack/react-query';
import { api, type Conversation } from '../lib/api';
import { useChatStore } from '../stores/chat';
import { useAuthStore } from '../stores/auth';

export default function Sidebar() {
  const { conversationId, setConversationId, reset } = useChatStore();
  const logout = useAuthStore((s) => s.logout);

  const { data: conversations } = useQuery({
    queryKey: ['conversations'],
    queryFn: () => api.listConversations(),
    refetchInterval: 10000,
  });

  const handleNew = () => {
    reset();
  };

  const handleSelect = (conv: Conversation) => {
    reset();
    setConversationId(conv.id);
    // Messages will be loaded by the chat view
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
          <button
            key={conv.id}
            onClick={() => handleSelect(conv)}
            className={`w-full text-left px-3 py-2 rounded-lg text-sm mb-0.5 truncate transition-colors ${
              conversationId === conv.id
                ? 'bg-zinc-800 text-white'
                : 'text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200'
            }`}
          >
            {conv.title || 'Untitled'}
          </button>
        ))}
      </div>

      <div className="p-3 border-t border-zinc-800">
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
