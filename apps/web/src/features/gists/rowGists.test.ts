import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { DaemonEvent } from "@/api/events";

const apiFetch = vi.fn<(path: string, opts?: unknown) => Promise<unknown>>();
let emit: (event: DaemonEvent) => void = () => {};
let setStatus: (status: { state: string }) => void = () => {};
vi.mock("@/api/client", () => ({
  apiFetch: (path: string, opts?: unknown) => apiFetch(path, opts),
}));
vi.mock("@/lib/ws", () => ({
  daemonEvents: {
    subscribe: (handler: (event: DaemonEvent) => void) => {
      emit = handler;
      return () => {
        emit = () => {};
      };
    },
    getStatus: () => ({ state: "connected" }),
    onStatus: (listener: (status: { state: string }) => void) => {
      setStatus = listener;
      return () => {
        setStatus = () => {};
      };
    },
  },
}));

const { getRowGist, requestRowGists, resetRowGistsForTest } = await import("./rowGists");

function gist(threadId: string, ask: string | null = "confirm the owner") {
  return {
    thread_id: threadId,
    status: "ready" as const,
    gist: "Canary stays at 5% until the dashboard is quiet.",
    ask: ask ? { summary: ask, quote: null } : null,
    provenance: { model: "gemma4", locality: "local" as const, sources: ["this_thread" as const] },
    from_cache: true,
    newest_message_id: "m1",
  };
}

function answer(gists: ReturnType<typeof gist>[], model = "available", queued: string[] = []) {
  return { kind: "ThreadGists", batch: { model, gists, queued, skipped: [] } };
}

beforeEach(() => {
  resetRowGistsForTest();
  apiFetch.mockReset();
});
afterEach(() => vi.useRealTimers());

describe("row gists", () => {
  test("asks for the rows in order, once, and keeps what the cache had", async () => {
    apiFetch.mockResolvedValue(answer([gist("b")], "available", ["a"]));
    await requestRowGists(["a", "b", "a"], 1_000);
    expect(apiFetch).toHaveBeenCalledTimes(1);
    expect(apiFetch.mock.calls[0]?.[1]).toEqual({
      method: "POST",
      body: { thread_ids: ["a", "b"], generate: true },
    });
    expect(getRowGist("b")).toEqual({
      about: "Canary stays at 5% until the dashboard is quiet.",
      ask: "confirm the owner",
      source: "Local model gemma4 · from this thread",
      provenance: { model: "gemma4", locality: "local", sources: ["this_thread"] },
      newest: "m1",
    });
    expect(getRowGist("a")).toBeUndefined();

    await requestRowGists(["a", "b"], 30_000);
    expect(apiFetch).toHaveBeenCalledTimes(1);
    apiFetch.mockResolvedValue(answer([]));
    await requestRowGists(["a", "b"], 62_000);
    expect(apiFetch.mock.calls[1]?.[1]).toMatchObject({ body: { thread_ids: ["a"] } });
  });

  test("missed events or a reconnect drop every line and let the rows ask again", async () => {
    apiFetch.mockResolvedValue(answer([gist("a")]));
    await requestRowGists(["a"], 1_000);
    expect(getRowGist("a")).toBeDefined();
    emit({ type: "EventsLagged", skipped: 3 } as unknown as DaemonEvent);
    expect(getRowGist("a")).toBeUndefined();
    await requestRowGists(["a"], 2_000);
    expect(apiFetch).toHaveBeenCalledTimes(2);

    setStatus({ state: "reconnecting" });
    setStatus({ state: "connected" });
    expect(getRowGist("a")).toBeUndefined();
    await requestRowGists(["a"], 3_000);
    expect(apiFetch).toHaveBeenCalledTimes(3);
  });
});
