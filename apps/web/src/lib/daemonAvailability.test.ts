import { onlineManager, QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { apiFetch, DaemonUnavailableError } from "@/api/client";

import {
  isDaemonDown,
  probeDaemon,
  refuseWhileDaemonDown,
  resetDaemonAvailabilityForTest,
  startDaemonAvailability,
} from "./daemonAvailability";

vi.mock("@/lib/localHandshake", () => ({
  tryLocalHandshake: vi.fn<() => Promise<string>>(async () => "test-token"),
}));

vi.mock("@/lib/tokenStorage", () => ({
  clearToken: vi.fn<() => void>(),
  getBridgeBaseUrl: () => "http://127.0.0.1:7777",
  getToken: () => "test-token",
}));

const socket = vi.hoisted(() => ({
  state: "connected" as string,
  reconnectNow: vi.fn<() => void>(),
  listener: undefined as ((status: { state: string }) => void) | undefined,
}));
vi.mock("@/lib/ws", () => ({
  daemonEvents: {
    getStatus: () => ({ state: socket.state }),
    onStatus: (listener: (status: { state: string }) => void) => {
      socket.listener = listener;
      return () => {
        socket.listener = undefined;
      };
    },
    reconnectNow: socket.reconnectNow,
  },
}));

const toast = vi.hoisted(() => ({ warning: vi.fn<(message: string, options?: object) => void>() }));
vi.mock("sonner", () => ({ toast }));

/** Every fetch answers from `daemon`: up (200) or gone (no response). */
let daemonUp = true;
const fetchMock = vi.fn<(input: string) => Promise<Response>>(async () => {
  if (!daemonUp) throw new TypeError("Failed to fetch");
  return new Response("{}", { status: 200 });
});

let stop: () => void;

beforeEach(() => {
  vi.useFakeTimers();
  daemonUp = true;
  fetchMock.mockClear();
  toast.warning.mockClear();
  socket.state = "connected";
  socket.reconnectNow.mockClear();
  vi.stubGlobal("fetch", fetchMock);
  resetDaemonAvailabilityForTest();
});

afterEach(() => {
  stop?.();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

async function settle(): Promise<void> {
  // Let the probe's fetch and its handlers run.
  await vi.advanceTimersByTimeAsync(0);
}

describe("daemon availability", () => {
  test("an unanswered request probes; only a failed probe marks the daemon down", async () => {
    stop = startDaemonAvailability(new QueryClient());
    daemonUp = false;
    await expect(apiFetch("/api/v1/mail/desk")).rejects.toBeInstanceOf(DaemonUnavailableError);
    await settle();
    expect(fetchMock.mock.calls.map(([url]) => url)).toContain(
      "http://127.0.0.1:7777/api/v1/admin/status",
    );
    expect(isDaemonDown()).toBe(true);
    // Queries pause (keeping their data) instead of failing.
    expect(onlineManager.isOnline()).toBe(false);
  });

  test("a probe the daemon answers, even with an error, keeps it up", async () => {
    stop = startDaemonAvailability(new QueryClient());
    fetchMock.mockImplementationOnce(async () => {
      throw new TypeError("Failed to fetch");
    });
    fetchMock.mockImplementationOnce(
      async () =>
        new Response(JSON.stringify({ error: "store busy", code: "store" }), { status: 500 }),
    );
    await expect(apiFetch("/api/v1/mail/desk")).rejects.toBeInstanceOf(DaemonUnavailableError);
    await settle();
    expect(isDaemonDown()).toBe(false);
    expect(onlineManager.isOnline()).toBe(true);
  });

  test("probes back off while down: a minute costs about ten requests, not hundreds", async () => {
    stop = startDaemonAvailability(new QueryClient());
    daemonUp = false;
    await probeDaemon();
    fetchMock.mockClear();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(isDaemonDown()).toBe(true);
    expect(fetchMock.mock.calls.length).toBeGreaterThanOrEqual(6);
    expect(fetchMock.mock.calls.length).toBeLessThanOrEqual(10);
  });

  test("coming back refetches what is shown, except open drafts, and reconnects events", async () => {
    const queryClient = new QueryClient();
    queryClient.setQueryData(["mailbox", "inbox"], { rows: [] });
    queryClient
      .getQueryCache()
      .build(queryClient, {
        queryKey: ["compose-session", "reply:1"],
        meta: { keepThroughGaps: true },
      });
    queryClient.setQueryData(["compose-session", "reply:1"], { body: "typed" });
    stop = startDaemonAvailability(queryClient);
    daemonUp = false;
    await probeDaemon();
    socket.state = "reconnecting";

    daemonUp = true;
    await vi.advanceTimersByTimeAsync(2_000);

    expect(isDaemonDown()).toBe(false);
    expect(onlineManager.isOnline()).toBe(true);
    expect(queryClient.getQueryState(["mailbox", "inbox"])?.isInvalidated).toBe(true);
    // Refetching it would put the saved file back over text typed since.
    expect(queryClient.getQueryState(["compose-session", "reply:1"])?.isInvalidated).toBe(false);
    expect(socket.reconnectNow).toHaveBeenCalledOnce();
  });

  test("the event stream dropping is enough to check, and its return rechecks at once", async () => {
    stop = startDaemonAvailability(new QueryClient());
    daemonUp = false;
    socket.listener?.({ state: "reconnecting" });
    await settle();
    expect(isDaemonDown()).toBe(true);

    daemonUp = true;
    socket.listener?.({ state: "connected" });
    await settle();
    expect(isDaemonDown()).toBe(false);
  });

  test("a change is refused, and said so, only while the daemon is down", async () => {
    stop = startDaemonAvailability(new QueryClient());
    expect(refuseWhileDaemonDown("archive")).toBe(false);
    expect(toast.warning).not.toHaveBeenCalled();

    daemonUp = false;
    await probeDaemon();
    expect(refuseWhileDaemonDown("archive")).toBe(true);
    expect(toast.warning).toHaveBeenCalledWith("Can't archive while mxr's daemon is stopped", {
      id: "daemon-down-refusal",
      description: "Nothing changed. Try again once it's back.",
    });
  });
});
