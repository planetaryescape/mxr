import { afterEach, describe, expect, test, vi } from "vitest";

import { daemonEvents, parseFrame } from "./ws";

vi.mock("@/lib/tokenStorage", () => ({
  getBridgeWsUrl: () => "ws://127.0.0.1:7777",
  getToken: () => "test-token",
  onTokenChange: () => () => {},
}));

describe("parseFrame", () => {
  test("a SyncError frame reaches the handlers as a sync failure, not a lost bridge", () => {
    const frame = parseFrame(
      JSON.stringify({
        event: "SyncError",
        account_id: "a-1",
        error: "Provider error: rate limited",
      }),
    );
    expect(frame).toEqual({
      kind: "event",
      event: {
        event: "SyncError",
        type: "SyncError",
        account_id: "a-1",
        error: "Provider error: rate limited",
      },
    });
  });

  test("an OperationFailed frame reaches the handlers too", () => {
    const frame = parseFrame(
      JSON.stringify({
        event: "OperationFailed",
        operation_id: "op-1",
        operation: "sync",
        account_id: "a-1",
        error: "connection refused",
        retryable: true,
      }),
    );
    expect(frame.kind).toBe("event");
    expect(frame.kind === "event" && frame.event.type).toBe("OperationFailed");
  });

  test("only a bare error frame means the bridge lost the daemon", () => {
    expect(parseFrame(JSON.stringify({ error: "daemon unreachable" }))).toEqual({
      kind: "bridge-error",
      error: "daemon unreachable",
    });
    expect(parseFrame("not json")).toEqual({ kind: "ignore" });
    expect(parseFrame(JSON.stringify({ hello: 1 }))).toEqual({ kind: "ignore" });
  });
});

describe("daemonEvents reconnect", () => {
  afterEach(() => {
    daemonEvents.stop();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  // A bridge that can't reach the daemon still accepts the upgrade, sends one
  // error frame and closes. Resetting the backoff on that open once turned
  // into a reconnect every 250ms.
  test("a stream that opens and closes at once backs off instead of retrying every 250ms", () => {
    vi.useFakeTimers();
    let attempts = 0;
    class BridgeWithoutDaemon extends EventTarget {
      static readonly OPEN = 1;
      readyState = 0;
      constructor() {
        super();
        attempts += 1;
        setTimeout(() => {
          this.readyState = 1;
          this.dispatchEvent(new Event("open"));
          const error = JSON.stringify({ error: "failed to connect to mxr daemon" });
          this.dispatchEvent(new MessageEvent("message", { data: error }));
          this.readyState = 3;
          this.dispatchEvent(Object.assign(new Event("close"), { code: 1011, reason: "" }));
        }, 0);
      }
      send(): void {}
      close(): void {}
    }
    vi.stubGlobal("WebSocket", BridgeWithoutDaemon);

    daemonEvents.start();
    vi.advanceTimersByTime(6_000);

    // 250ms doubling with jitter: about five tries in six seconds, not 24.
    expect(attempts).toBeGreaterThan(1);
    expect(attempts).toBeLessThanOrEqual(7);
    expect(daemonEvents.getStatus().state).toBe("reconnecting");
  });
});
