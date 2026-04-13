import { useRef, useCallback, useEffect } from 'react';
import { useChatStore } from '../stores/chat';
import { useAuthStore } from '../stores/auth';

interface WsMessage {
  type: string;
  text?: string;
  tool?: string;
  status?: 'started' | 'completed' | 'failed';
  result?: string;
  conversation_id?: string;
  error?: string;
}

export function useWebSocket() {
  const wsRef = useRef<WebSocket | null>(null);
  const token = useAuthStore((s) => s.token);
  const {
    appendStreamText,
    finalizeAssistantMessage,
    setToolStatus,
    clearActiveTools,
    setStreaming,
    setConversationId,
    setError,
  } = useChatStore();

  const connect = useCallback(() => {
    if (!token || wsRef.current?.readyState === WebSocket.OPEN) return;

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws?token=${token}`);
    wsRef.current = ws;

    ws.onmessage = (event) => {
      const msg: WsMessage = JSON.parse(event.data);

      switch (msg.type) {
        case 'text':
          appendStreamText(msg.text ?? '');
          break;
        case 'tool_status':
          setToolStatus({
            tool: msg.tool ?? 'unknown',
            status: msg.status ?? 'started',
            result: msg.result,
          });
          break;
        case 'done':
          finalizeAssistantMessage();
          clearActiveTools();
          break;
        case 'conversation_created':
          if (msg.conversation_id) setConversationId(msg.conversation_id);
          break;
        case 'error':
          setError(msg.error ?? 'Unknown error');
          setStreaming(false);
          break;
      }
    };

    ws.onerror = () => {
      setError('WebSocket connection error');
      setStreaming(false);
    };

    ws.onclose = () => {
      wsRef.current = null;
    };
  }, [token, appendStreamText, finalizeAssistantMessage, setToolStatus, clearActiveTools, setStreaming, setConversationId, setError]);

  const sendMessage = useCallback(
    (content: string, conversationId?: string | null) => {
      if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
        connect();
        // Retry after connection opens
        setTimeout(() => sendMessage(content, conversationId), 500);
        return;
      }

      setStreaming(true);
      setError(null);
      clearActiveTools();

      wsRef.current.send(
        JSON.stringify({
          type: 'chat',
          content,
          conversation_id: conversationId ?? undefined,
        })
      );
    },
    [connect, setStreaming, setError, clearActiveTools]
  );

  const disconnect = useCallback(() => {
    wsRef.current?.close();
    wsRef.current = null;
  }, []);

  // Auto-connect when token available
  useEffect(() => {
    if (token) connect();
    return () => disconnect();
  }, [token, connect, disconnect]);

  return { sendMessage, connect, disconnect, isConnected: !!wsRef.current };
}
