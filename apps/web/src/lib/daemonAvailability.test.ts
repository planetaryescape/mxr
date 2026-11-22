import { onlineManager, QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, test, vi, type Mock } from "vitest";

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

interface AvailabilitySocket {
  state: string;
  reconnectNow: Mock<() => void>;
  eventListener: ((event: { type: string; deadline_at: number }) => void) | undefined;
  listener: ((status: { state: string }) => void) | undefined;
}

const socket = vi.hoisted<AvailabilitySocket>(() => ({
  state: "connected",
  reconnectNow: vi.fn<() => void>(),
  eventListener: undefined,
  listener: undefined,
}));
vi.mock("@/lib/ws", () => ({
  daemonEvents: {
    getStatus: () => ({ state: socket.state }),
    subscribe: (listener: (event: { type: string; deadline_at: number }) => void) => {
      socket.eventListener = listener;
      return () => {
        socket.eventListener = undefined;
      };
    },
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
  await vi.advanceTimersByTimeAsync(0);
}

describe("daemon availability", () => {
  test("a worker offline response is not an answer from a live daemon", async () => {
    stop = startDaemonAvailability(new QueryClient());
    fetchMock.mockImplementationOnce(async () =>
      Response.json(
        { error: "not saved", code: "offline_not_cached" },
        { status: 503, headers: { "x-mxr-offline": "1" } },
      ),
    );
    await probeDaemon();
    await settle();
    expect(isDaemonDown()).toBe(true);
    expect(onlineManager.isOnline()).toBe(false);
  });

  test("an unanswered request probes; only a failed probe marks the daemon down", async () => {
    stop = startDaemonAvailability(new QueryClient());
    daemonUp = false;
    await expect(apiFetch("/api/v1/mail/desk")).rejects.toBeInstanceOf(DaemonUnavailableError);
    await settle();
    expect(fetchMock.mock.calls.map(([url]) => url)).toContain(
      "http://127.0.0.1:7777/api/v1/admin/status",
    );
    expect(isDaemonDown()).toBe(true);
    expect(onlineManager.isOnline()).toBe(false);
  });

  test("a change is refused, and said so, only while the daemon is down", async () => {
    stop = startDaemonAvailability(new QueryClient());
    expect(refuseWhileDaemonDown("archive")).toBe(false);
    expect(toast.warning).not.toHaveBeenCalled();

    daemonUp = false;
    await probeDaemon();
    expect(refuseWhileDaemonDown("archive")).toBe(true);
    expect(toast.warning).toHaveBeenCalledWith("Can't archive while mxr is unavailable", {
      id: "daemon-down-refusal",
      description: "Nothing changed. Try again once it's back.",
    });
  });
});
