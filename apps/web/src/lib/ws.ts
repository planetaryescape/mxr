import type { DaemonEvent, DaemonEventHandler } from "@/api/events";
import { getBridgeWsUrl, getToken, onTokenChange } from "@/lib/tokenStorage";

export type ConnectionState =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "offline"
  | "unauthorized";

export interface ConnectionStatus {
  state: ConnectionState;
  lastEventAt?: number;
  lastErrorAt?: number;
  errorMessage?: string;
}

type StatusListener = (status: ConnectionStatus) => void;

const HEARTBEAT_MS = 25_000;
const MIN_BACKOFF_MS = 250;
const MAX_BACKOFF_MS = 30_000;
const STABLE_CONNECTION_MS = 5_000;
const PATH = "/api/v1/events";

class DaemonEventClient {
  private socket?: WebSocket;
  private handlers = new Set<DaemonEventHandler>();
  private statusListeners = new Set<StatusListener>();
  private status: ConnectionStatus = { state: "offline" };
  private heartbeatHandle?: ReturnType<typeof setInterval>;
  private retryHandle?: ReturnType<typeof setTimeout>;
  private retryAttempt = 0;
  private openedAt?: number;
  private wantOpen = false;
  private unsubscribeToken: (() => void) | undefined;

  start(): void {
    this.wantOpen = true;
    this.unsubscribeToken?.();
    this.unsubscribeToken = onTokenChange(() => {
      if (this.wantOpen && this.status.state !== "connected") this.reconnectNow();
    });
    this.openSocket();
    if (typeof window !== "undefined") {
      window.addEventListener("online", this.onOnline);
      window.addEventListener("offline", this.onOffline);
      window.addEventListener("focus", this.onFocus);
    }
  }

  stop(): void {
    this.wantOpen = false;
    this.unsubscribeToken?.();
    this.unsubscribeToken = undefined;
    if (typeof window !== "undefined") {
      window.removeEventListener("online", this.onOnline);
      window.removeEventListener("offline", this.onOffline);
      window.removeEventListener("focus", this.onFocus);
    }
    this.clearTimers();
    this.closeSocket();
    this.setStatus({ state: "offline" });
  }

  reconnectNow(): void {
    this.clearTimers();
    this.closeSocket();
    this.retryAttempt = 0;
    this.openSocket();
  }

  subscribe(handler: DaemonEventHandler): () => void {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }

  onStatus(listener: StatusListener): () => void {
    this.statusListeners.add(listener);
    listener(this.status);
    return () => this.statusListeners.delete(listener);
  }

  getStatus(): ConnectionStatus {
    return this.status;
  }

  private openSocket(): void {
    const token = getToken();
    if (!token) {
      this.setStatus({ state: "unauthorized" });
      return;
    }
    const url = getBridgeWsUrl() + PATH;
    this.setStatus({ state: this.retryAttempt > 0 ? "reconnecting" : "connecting" });
    try {
      const ws = new WebSocket(url, ["bearer", token]);
      this.socket = ws;
      ws.addEventListener("open", this.onOpen);
      ws.addEventListener("message", this.onMessage);
      ws.addEventListener("close", this.onClose);
      ws.addEventListener("error", this.onError);
    } catch (err) {
      this.scheduleReconnect((err as Error).message);
    }
  }

  private onOpen = (): void => {
    this.openedAt = Date.now();
    this.setStatus({ state: "connected", lastEventAt: this.status.lastEventAt });
    this.heartbeatHandle = setInterval(() => {
      if (this.socket && this.socket.readyState === WebSocket.OPEN) {
        try {
          this.socket.send(JSON.stringify({ type: "ping" }));
        } catch {}
      }
    }, HEARTBEAT_MS);
  };

  private onMessage = (ev: MessageEvent): void => {
    if (typeof ev.data !== "string") return;
    const frame = parseFrame(ev.data);
    if (frame.kind === "ignore") return;
    if (frame.kind === "bridge-error") {
      this.setStatus({
        state: "reconnecting",
        errorMessage: frame.error,
        lastErrorAt: Date.now(),
      });
      return;
    }
    this.setStatus({ state: "connected", lastEventAt: Date.now() });
    for (const handler of this.handlers) {
      try {
        handler(frame.event);
      } catch (err) {
        console.error("[mxr/ws] event handler threw", err);
      }
    }
  };

  private onClose = (ev: CloseEvent): void => {
    this.clearHeartbeat();
    if (this.openedAt !== undefined && Date.now() - this.openedAt >= STABLE_CONNECTION_MS) {
      this.retryAttempt = 0;
    }
    this.openedAt = undefined;
    if (ev.code === 4401 || ev.code === 1008) {
      this.setStatus({ state: "unauthorized", errorMessage: ev.reason });
      return;
    }
    this.scheduleReconnect(ev.reason || `closed (${ev.code})`);
  };

  private onError = (): void => {};

  private onOnline = (): void => {
    this.reconnectNow();
  };

  private onOffline = (): void => {
    this.setStatus({ state: "offline" });
    this.closeSocket();
  };

  private onFocus = (): void => {
    if (this.status.state !== "connected") this.reconnectNow();
  };

  private scheduleReconnect(reason?: string): void {
    if (!this.wantOpen) return;
    this.retryAttempt += 1;
    const base = Math.min(MAX_BACKOFF_MS, MIN_BACKOFF_MS * 2 ** (this.retryAttempt - 1));
    const jitter = Math.random() * 0.3 * base;
    const delay = Math.floor(base + jitter);
    this.setStatus({
      state: "reconnecting",
      errorMessage: reason,
      lastErrorAt: Date.now(),
    });
    this.retryHandle = setTimeout(() => this.openSocket(), delay);
  }

  private closeSocket(): void {
    if (this.socket) {
      this.socket.removeEventListener("open", this.onOpen);
      this.socket.removeEventListener("message", this.onMessage);
      this.socket.removeEventListener("close", this.onClose);
      this.socket.removeEventListener("error", this.onError);
      try {
        this.socket.close();
      } catch {}
      this.socket = undefined;
    }
  }

  private clearHeartbeat(): void {
    if (this.heartbeatHandle) {
      clearInterval(this.heartbeatHandle);
      this.heartbeatHandle = undefined;
    }
  }

  private clearTimers(): void {
    this.clearHeartbeat();
    if (this.retryHandle) {
      clearTimeout(this.retryHandle);
      this.retryHandle = undefined;
    }
  }

  private setStatus(next: Partial<ConnectionStatus> & Pick<ConnectionStatus, "state">): void {
    this.status = { ...this.status, ...next };
    for (const listener of this.statusListeners) {
      listener(this.status);
    }
  }
}

export type Frame =
  | { kind: "event"; event: DaemonEvent }
  | { kind: "bridge-error"; error: string }
  | { kind: "ignore" };

export function parseFrame(data: string): Frame {
  let parsed: unknown;
  try {
    parsed = JSON.parse(data);
  } catch {
    return { kind: "ignore" };
  }
  if (!parsed || typeof parsed !== "object") return { kind: "ignore" };
  const frame = parsed as Record<string, unknown>;
  const named = "type" in frame || "event" in frame;
  if (!named && typeof frame.error === "string")
    return { kind: "bridge-error", error: frame.error };
  if (!("type" in frame) && typeof frame.event === "string") frame.type = frame.event;
  if (typeof frame.type !== "string") return { kind: "ignore" };
  return { kind: "event", event: frame as DaemonEvent };
}

export const daemonEvents = new DaemonEventClient();
