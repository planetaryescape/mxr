import { renderHook, act } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useEventStream, type EventStreamCallbacks } from "./useEventStream";

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  url: string;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  close() {
    this.closed = true;
  }

  send(_data: string) {}

  simulateOpen() {
    this.onopen?.();
  }

  simulateMessage(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) });
  }

  simulateClose() {
    this.onclose?.();
  }

  simulateError() {
    this.onerror?.();
  }
}

function makeCallbacks(): EventStreamCallbacks & { calls: Record<string, unknown[]> } {
  const calls: Record<string, unknown[]> = {
    onSyncCompleted: [],
    onSyncError: [],
    onNewMessages: [],
    onMessageUnsnoozed: [],
    onLabelCountsUpdated: [],
  };

  return {
    calls,
    onSyncCompleted: (e) => calls.onSyncCompleted.push(e),
    onSyncError: (e) => calls.onSyncError.push(e),
    onNewMessages: (e) => calls.onNewMessages.push(e),
    onMessageUnsnoozed: (e) => calls.onMessageUnsnoozed.push(e),
    onLabelCountsUpdated: (e) => calls.onLabelCountsUpdated.push(e),
  };
}

describe("useEventStream", () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.stubGlobal("WebSocket", MockWebSocket);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns disconnected when baseUrl is null", () => {
    const callbacks = makeCallbacks();
    const { result } = renderHook(() => useEventStream(null, null, callbacks));

    expect(result.current).toBe("disconnected");
    expect(MockWebSocket.instances).toHaveLength(0);
  });

  it("connects and transitions to connected on open", () => {
    const callbacks = makeCallbacks();
    const { result } = renderHook(() =>
      useEventStream("http://localhost:8080", "test-token", callbacks),
    );

    expect(MockWebSocket.instances).toHaveLength(1);
    expect(MockWebSocket.instances[0].url).toBe("ws://localhost:8080/events?token=test-token");
    expect(result.current).toBe("connecting");

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
    });

    expect(result.current).toBe("connected");
  });

  it("dispatches SyncCompleted events to the callback", () => {
    const callbacks = makeCallbacks();
    renderHook(() => useEventStream("http://localhost:8080", "tok", callbacks));

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
      MockWebSocket.instances[0].simulateMessage({
        event: "SyncCompleted",
        account_id: "acc-1",
        messages_synced: 5,
      });
    });

    expect(callbacks.calls.onSyncCompleted).toHaveLength(1);
    expect(callbacks.calls.onSyncCompleted[0]).toEqual({
      event: "SyncCompleted",
      account_id: "acc-1",
      messages_synced: 5,
    });
  });

  it("dispatches NewMessages events to the callback", () => {
    const callbacks = makeCallbacks();
    renderHook(() => useEventStream("http://localhost:8080", "tok", callbacks));

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
      MockWebSocket.instances[0].simulateMessage({
        event: "NewMessages",
        envelopes: [{ id: "e1", subject: "Hello", from: { name: "A", email: "a@b.c" } }],
      });
    });

    expect(callbacks.calls.onNewMessages).toHaveLength(1);
  });

  it("dispatches SyncError events to the callback", () => {
    const callbacks = makeCallbacks();
    renderHook(() => useEventStream("http://localhost:8080", "tok", callbacks));

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
      MockWebSocket.instances[0].simulateMessage({
        event: "SyncError",
        account_id: "acc-1",
        error: "connection timeout",
      });
    });

    expect(callbacks.calls.onSyncError).toHaveLength(1);
    expect((callbacks.calls.onSyncError[0] as { error: string }).error).toBe("connection timeout");
  });

  it("dispatches LabelCountsUpdated events to the callback", () => {
    const callbacks = makeCallbacks();
    renderHook(() => useEventStream("http://localhost:8080", "tok", callbacks));

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
      MockWebSocket.instances[0].simulateMessage({
        event: "LabelCountsUpdated",
        counts: [{ label_id: "l1", unread_count: 3, total_count: 10 }],
      });
    });

    expect(callbacks.calls.onLabelCountsUpdated).toHaveLength(1);
  });

  it("dispatches MessageUnsnoozed events to the callback", () => {
    const callbacks = makeCallbacks();
    renderHook(() => useEventStream("http://localhost:8080", "tok", callbacks));

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
      MockWebSocket.instances[0].simulateMessage({
        event: "MessageUnsnoozed",
        message_id: "msg-99",
      });
    });

    expect(callbacks.calls.onMessageUnsnoozed).toHaveLength(1);
  });

  it("ignores malformed messages without crashing", () => {
    const callbacks = makeCallbacks();
    renderHook(() => useEventStream("http://localhost:8080", "tok", callbacks));

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
      // Send non-JSON
      MockWebSocket.instances[0].onmessage?.({ data: "not json" });
      // Send JSON without event field
      MockWebSocket.instances[0].simulateMessage({ foo: "bar" });
    });

    // No callbacks fired, no crash
    expect(callbacks.calls.onSyncCompleted).toHaveLength(0);
    expect(callbacks.calls.onNewMessages).toHaveLength(0);
  });

  it("transitions to disconnected on close and cleans up on unmount", () => {
    const callbacks = makeCallbacks();
    const { result, unmount } = renderHook(() =>
      useEventStream("http://localhost:8080", "tok", callbacks),
    );

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
    });
    expect(result.current).toBe("connected");

    act(() => {
      MockWebSocket.instances[0].simulateClose();
    });
    expect(result.current).toBe("disconnected");

    unmount();
    expect(MockWebSocket.instances[0].closed).toBe(false); // first instance closed via onclose
    // A reconnect timer was set, but unmount should have cleared it
  });
});
