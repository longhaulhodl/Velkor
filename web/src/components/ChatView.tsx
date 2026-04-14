import { useState, useRef, useEffect, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import { useChatStore } from '../stores/chat';
import { useWebSocket } from '../hooks/useWebSocket';
import { api, type DocumentMeta } from '../lib/api';
import ToolIndicator from './ToolIndicator';
import FileUpload from './FileUpload';

export default function ChatView() {
  const [input, setInput] = useState('');
  const [showUpload, setShowUpload] = useState(false);
  const [pendingDocs, setPendingDocs] = useState<DocumentMeta[]>([]);

  const handleDocUploaded = useCallback((doc: DocumentMeta) => {
    setPendingDocs((prev) => [...prev, doc]);
  }, []);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const {
    messages,
    streamingText,
    isStreaming,
    activeTools,
    conversationId,
    error,
    addUserMessage,
    loadMessages,
  } = useChatStore();
  const { sendMessage } = useWebSocket();

  // Load messages when mounting with a saved conversationId
  const loadedRef = useRef<string | null>(null);
  useEffect(() => {
    if (conversationId && conversationId !== loadedRef.current && messages.length === 0) {
      loadedRef.current = conversationId;
      api.getConversation(conversationId).then((detail) => {
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
      }).catch((e) => console.error('[Velkor] Failed to load conversation:', e));
    }
  }, [conversationId]);

  // Auto-scroll on new content
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, streamingText, activeTools]);

  const handleSend = () => {
    const text = input.trim();
    if (!text) return;

    // If documents were uploaded, prepend references so the model knows about them
    let fullMessage = text;
    if (pendingDocs.length > 0) {
      const docList = pendingDocs
        .map((d) => `- "${d.filename}" (id: ${d.id})`)
        .join('\n');
      fullMessage = `[Attached documents — use document_read to access their content]\n${docList}\n\n${text}`;
      setPendingDocs([]);
      setShowUpload(false);
    }

    addUserMessage(text); // Show the original text to the user
    sendMessage(fullMessage, conversationId);
    setInput('');
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="flex-1 flex flex-col h-screen bg-zinc-950">
      {/* Messages area */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-3xl mx-auto py-8 px-4">
          {messages.length === 0 && !isStreaming && (
            <div className="text-center pt-32">
              <h2 className="text-xl text-zinc-400 font-light">What can I help you with?</h2>
            </div>
          )}

          {messages.map((msg) => (
            <div key={msg.id} className={`mb-6 ${msg.role === 'user' ? 'flex justify-end' : ''}`}>
              {msg.role === 'user' ? (
                <div className="bg-zinc-800 rounded-2xl px-4 py-2.5 max-w-[80%] text-sm text-white whitespace-pre-wrap">
                  {msg.content}
                </div>
              ) : (
                <div className="prose prose-invert prose-sm max-w-none text-zinc-300">
                  <ReactMarkdown>{msg.content}</ReactMarkdown>
                </div>
              )}
            </div>
          ))}

          {/* Tool status indicators */}
          {activeTools.length > 0 && (
            <div className="mb-4 space-y-1">
              {activeTools.map((t) => (
                <ToolIndicator key={t.tool} tool={t.tool} status={t.status} />
              ))}
            </div>
          )}

          {/* Streaming text */}
          {streamingText && (
            <div className="mb-6 prose prose-invert prose-sm max-w-none text-zinc-300">
              <ReactMarkdown>{streamingText}</ReactMarkdown>
              <span className="inline-block w-1.5 h-4 bg-zinc-400 animate-pulse ml-0.5 align-text-bottom" />
            </div>
          )}

          {error && (
            <div className="mb-4 text-red-400 text-sm bg-red-950/30 border border-red-900 rounded-lg px-3 py-2">
              {error}
            </div>
          )}

          <div ref={messagesEndRef} />
        </div>
      </div>

      {/* File upload panel */}
      {showUpload && (
        <FileUpload onClose={() => setShowUpload(false)} onUploaded={handleDocUploaded} />
      )}

      {/* Pending document badges */}
      {pendingDocs.length > 0 && (
        <div className="border-t border-zinc-800 px-4 pt-2">
          <div className="max-w-3xl mx-auto flex flex-wrap gap-1.5">
            {pendingDocs.map((doc) => (
              <span
                key={doc.id}
                className="inline-flex items-center gap-1 text-xs bg-zinc-800 text-zinc-300 rounded-md px-2 py-1"
              >
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <polyline points="14 2 14 8 20 8" />
                </svg>
                {doc.filename}
                <button
                  onClick={() => setPendingDocs((prev) => prev.filter((d) => d.id !== doc.id))}
                  className="text-zinc-500 hover:text-zinc-300 ml-0.5"
                >
                  &times;
                </button>
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Input area */}
      <div className="border-t border-zinc-800 p-4">
        <div className="max-w-3xl mx-auto flex gap-2 items-end">
          <button
            onClick={() => setShowUpload(!showUpload)}
            title="Upload document"
            className="p-2.5 text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 rounded-lg transition-colors self-end"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
            </svg>
          </button>
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Send a message..."
            rows={1}
            className="flex-1 resize-none bg-zinc-900 border border-zinc-700 rounded-xl px-4 py-3 text-sm text-white placeholder-zinc-500 focus:outline-none focus:border-zinc-500 min-h-[44px] max-h-[200px]"
            style={{
              height: 'auto',
              overflow: input.split('\n').length > 1 ? 'auto' : 'hidden',
            }}
            onInput={(e) => {
              const el = e.target as HTMLTextAreaElement;
              el.style.height = 'auto';
              el.style.height = Math.min(el.scrollHeight, 200) + 'px';
            }}
          />
          <button
            onClick={handleSend}
            disabled={!input.trim()}
            className="px-4 py-2 bg-white text-black rounded-xl text-sm font-medium hover:bg-zinc-200 disabled:opacity-30 disabled:cursor-not-allowed transition-colors self-end"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
