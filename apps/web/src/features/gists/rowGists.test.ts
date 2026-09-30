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

const { getRowGist, putGist, requestRowGists, resetRowGistsForTest, toRowGist } =
  await import("./rowGists");

function gist(threadId: string, ask: string | null = "confirm the owner") {
  return {
    thread_id: threadId,
    status: "ready" as const,
    gist: "Canary stays at 5% until the dashboard is quiet.",
    ask: ask ? { summary: ask, quote: null } : null,
    provenance: { model: "gemma4", locality: "local" as const, sources: ["this_thread" as const] },
    from_cache: true,
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
    });
    expect(getRowGist("a")).toBeUndefined();

    // Scrolling back and forth asks for nothing new for a minute.
    await requestRowGists(["a", "b"], 30_000);
    expect(apiFetch).toHaveBeenCalledTimes(1);
    // After that, the rows still without a gist are asked about again.
    apiFetch.mockResolvedValue(answer([]));
    await requestRowGists(["a", "b"], 62_000);
    expect(apiFetch.mock.calls[1]?.[1]).toMatchObject({ body: { thread_ids: ["a"] } });
  });

  test("a queued gist lands from the daemon's event", async () => {
    apiFetch.mockResolvedValue(answer([], "available", ["a"]));
    await requestRowGists(["a"], 1_000);
    emit({ type: "ThreadGistReady", gist: gist("a", null) } as unknown as DaemonEvent);
    expect(getRowGist("a")?.ask).toBeNull();
    // A new message in the conversation retires its gist.
    emit({ type: "NewMessages", envelopes: [{ thread_id: "a" }] } as unknown as DaemonEvent);
    expect(getRowGist("a")).toBeUndefined();
  });

  test("with no model the list stops asking, and nothing is shown", async () => {
    apiFetch.mockResolvedValue(answer([], "disabled"));
    await requestRowGists(["a"], 1_000);
    await requestRowGists(["b"], 2_000);
    expect(apiFetch).toHaveBeenCalledTimes(1);
    expect(getRowGist("a")).toBeUndefined();
  });

  test("a failed request is forgotten so the next scroll asks again", async () => {
    apiFetch.mockRejectedValueOnce(new Error("offline"));
    await requestRowGists(["a"], 1_000);
    apiFetch.mockResolvedValue(answer([gist("a")]));
    await requestRowGists(["a"], 2_000);
    expect(apiFetch).toHaveBeenCalledTimes(2);
    expect(getRowGist("a")).toBeDefined();
  });

  test("a stale gist the cache no longer holds is dropped, not kept", async () => {
    putGist(gist("a"), 0);
    apiFetch.mockResolvedValue(answer([], "available", ["a"]));
    await requestRowGists(["a"], 6 * 60_000);
    expect(getRowGist("a")).toBeUndefined();
  });

  test("only ready gists with text become lines", () => {
    expect(toRowGist({ ...gist("a"), status: "failed" })).toBeNull();
    expect(toRowGist({ ...gist("a"), gist: "  " })).toBeNull();
  });

  test("a gist for an older message than one seen arriving is ignored", async () => {
    apiFetch.mockResolvedValue(answer([], "available", ["a"]));
    await requestRowGists(["a"], 1_000);
    emit({
      type: "NewMessages",
      envelopes: [{ thread_id: "a", id: "m2" }],
    } as unknown as DaemonEvent);
    const late = { ...gist("a"), newest_message_id: "m1" };
    emit({ type: "ThreadGistReady", gist: late } as unknown as DaemonEvent);
    expect(getRowGist("a")).toBeUndefined();
    emit({
      type: "ThreadGistReady",
      gist: { ...gist("a"), newest_message_id: "m2" },
    } as unknown as DaemonEvent);
    expect(getRowGist("a")).toBeDefined();
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
