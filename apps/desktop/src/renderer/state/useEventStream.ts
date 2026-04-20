import { useCallback, useEffect, useRef, useState } from "react";

export type ConnectionStatus = "connected" | "connecting" | "disconnected";

interface SyncCompletedEvent {
  event: "SyncCompleted";
  account_id: string;
  messages_synced: number;
}

interface SyncErrorEvent {
  event: "SyncError";
  account_id: string;
  error: string;
}

interface NewMessagesEvent {
  event: "NewMessages";
  envelopes: Array<{
    id: string;
    subject: string;
    from: { name: string; email: string };
  }>;
}

interface MessageUnsnoozedEvent {
  event: "MessageUnsnoozed";
  message_id: string;
}

interface LabelCountsUpdatedEvent {
  event: "LabelCountsUpdated";
  counts: Array<{
    label_id: string;
    unread_count: number;
    total_count: number;
  }>;
}

type DaemonEvent =
  | SyncCompletedEvent
  | SyncErrorEvent
  | NewMessagesEvent
  | MessageUnsnoozedEvent
  | LabelCountsUpdatedEvent;

export interface EventStreamCallbacks {
  onSyncCompleted: (event: SyncCompletedEvent) => void;
  onSyncError: (event: SyncErrorEvent) => void;
  onNewMessages: (event: NewMessagesEvent) => void;
  onMessageUnsnoozed: (event: MessageUnsnoozedEvent) => void;
  onLabelCountsUpdated: (event: LabelCountsUpdatedEvent) => void;
}

const MAX_BACKOFF = 30_000;

export function useEventStream(
  baseUrl: string | null,
  authToken: string | null,
  callbacks: EventStreamCallbacks,
): ConnectionStatus {
  const [status, setStatus] = useState<ConnectionStatus>("disconnected");
  const wsRef = useRef<WebSocket | null>(null);
  const retriesRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const callbacksRef = useRef(callbacks);
  callbacksRef.current = callbacks;

  const connect = useCallback(() => {
    if (!baseUrl || !authToken || typeof WebSocket === "undefined") {
      setStatus("disconnected");
      return;
    }

    // Convert http:// to ws://
    const wsUrl = baseUrl.replace(/^http/, "ws") + `/events?token=${authToken}`;

    setStatus("connecting");
    let ws: WebSocket;
    try {
      ws = new WebSocket(wsUrl);
    } catch {
      // WebSocket not available (e.g. test environment)
      return;
    }
    wsRef.current = ws;

    ws.onopen = () => {
      setStatus("connected");
      retriesRef.current = 0;
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as DaemonEvent;
        switch (data.event) {
          case "SyncCompleted":
            callbacksRef.current.onSyncCompleted(data);
            break;
          case "SyncError":
            callbacksRef.current.onSyncError(data);
            break;
          case "NewMessages":
            callbacksRef.current.onNewMessages(data);
            break;
          case "MessageUnsnoozed":
            callbacksRef.current.onMessageUnsnoozed(data);
            break;
          case "LabelCountsUpdated":
            callbacksRef.current.onLabelCountsUpdated(data);
            break;
        }
      } catch {
        // Ignore malformed messages
      }
    };

    ws.onclose = () => {
      setStatus("disconnected");
      wsRef.current = null;
      // Exponential backoff reconnect, cap at 10 retries
      if (retriesRef.current < 10) {
        const delay = Math.min(1000 * 2 ** retriesRef.current, MAX_BACKOFF);
        retriesRef.current++;
        timerRef.current = setTimeout(connect, delay);
      }
    };

    ws.onerror = () => {
      // onclose will fire after onerror
    };
  }, [baseUrl, authToken]);

  useEffect(() => {
    connect();
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      if (wsRef.current) {
        wsRef.current.onclose = null; // Prevent reconnect on intentional close
        wsRef.current.close();
        wsRef.current = null;
      }
    };
  }, [connect]);

  return status;
}
