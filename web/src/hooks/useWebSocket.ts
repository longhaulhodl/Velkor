import { useRef, useCallback, useEffect } from 'react';
import { useChatStore } from '../stores/chat';
import { useAuthStore } from '../stores/auth';
import { useNotificationStore } from '../stores/notifications';

interface WsMessage {
  type: string;
  text?: string;
  content?: string;
  tool?: string;
  status?: 'started' | 'completed' | 'failed';
  result?: string;
  conversation_id?: string;
  error?: string;
  message?: string;
  position?: number;
  // task_complete fields
  task_id?: string;
  title?: string;
  result_summary?: string;
  tokens_used?: number;
}

export function useWebSocket() {
  const wsRef = useRef<WebSocket | null>(null);
  const pendingRef = useRef<string | null>(null);
  const token = useAuthStore((s) => s.token);
  const {
    appendStreamText,
    finalizeAssistantMessage,
    setToolStatus,
    clearActiveTools,
    setStreaming,
    setConversationId,
    setError,
    isStreaming,
  } = useChatStore();

  const addTaskNotification = useNotificationStore((s) => s.addTaskNotification);

  const connect = useCallback(() => {
    if (!token) return;

    // Already open or connecting — don't create a second socket
    if (
      wsRef.current &&
      (wsRef.current.readyState === WebSocket.OPEN ||
        wsRef.current.readyState === WebSocket.CONNECTING)
    ) {
      return;
    }

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws?token=${token}`);
    wsRef.current = ws;

    ws.onopen = () => {
      console.log('[Velkor] WebSocket connected');
      // If there's a pending message queued before the socket opened, send it now
      if (pendingRef.current) {
        ws.send(pendingRef.current);
        pendingRef.current = null;
      }
    };

    ws.onmessage = (event) => {
      let msg: WsMessage;
      try {
        msg = JSON.parse(event.data);
      } catch {
        return;
      }

      switch (msg.type) {
        case 'text':
          appendStreamText(msg.text ?? msg.content ?? '');
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
        case 'task_complete':
          addTaskNotification({
            task_id: msg.task_id!,
            title: msg.title ?? 'Background task',
            status: msg.status ?? 'completed',
            result_summary: msg.result_summary,
            conversation_id: msg.conversation_id,
            error: msg.error,
            tokens_used: msg.tokens_used ?? 0,
          });
          console.log(`[Velkor] Task completed: ${msg.title} (${msg.status})`);
          break;
        case 'queued':
          // Message was queued server-side — keep isStreaming true, nothing to reset
          console.log(`[Velkor] Message queued at position ${msg.position}`);
          break;
        case 'error':
          setError(msg.error ?? msg.message ?? 'Unknown error');
          setStreaming(false);
          break;
      }
    };

    ws.onerror = (event) => {
      console.error('[Velkor] WebSocket error:', event);
      setError('WebSocket connection error');
      setStreaming(false);
    };

    ws.onclose = (event) => {
      console.log(`[Velkor] WebSocket closed: code=${event.code} reason=${event.reason}`);
      wsRef.current = null;
    };
  }, [token, appendStreamText, finalizeAssistantMessage, setToolStatus, clearActiveTools, setStreaming, setConversationId, setError, addTaskNotification]);

  const sendMessage = useCallback(
    (content: string, conversationId?: string | null) => {
      const payload = JSON.stringify({
        type: 'chat',
        content,
        conversation_id: conversationId ?? undefined,
      });

      // Only reset streaming state if we're not currently streaming.
      // If we ARE streaming, the server will queue this message and
      // process it after the current response finishes — we don't want
      // to wipe the in-progress streaming text.
      if (!isStreaming) {
        setStreaming(true);
        setError(null);
        clearActiveTools();
      }

      const ws = wsRef.current;

      if (ws && ws.readyState === WebSocket.OPEN) {
        // Socket is ready — send immediately
        ws.send(payload);
      } else if (ws && ws.readyState === WebSocket.CONNECTING) {
        // Socket is still connecting — queue the message for onopen
        console.log('[Velkor] WebSocket connecting, queuing message...');
        pendingRef.current = payload;
      } else {
        // No socket — connect and queue
        console.log('[Velkor] WebSocket not connected, connecting and queuing message...');
        pendingRef.current = payload;
        connect();
      }
    },
    [connect, setStreaming, setError, clearActiveTools, isStreaming]
  );

  const disconnect = useCallback(() => {
    wsRef.current?.close();
    wsRef.current = null;
  }, []);

  // Auto-connect when token is available
  useEffect(() => {
    if (token) connect();
    return () => disconnect();
  }, [token, connect, disconnect]);

  return { sendMessage, connect, disconnect, isConnected: !!wsRef.current };
}
