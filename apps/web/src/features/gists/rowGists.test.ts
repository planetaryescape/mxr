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

const {
  forgetGists,
  getRowGist,
  putGist,
  requestRowGists,
  resetRowGistsForTest,
  STATE_CAP,
  toRowGist,
} = await import("./rowGists");

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

  test("only ready gists with text become lines", () => {
    expect(toRowGist({ ...gist("a"), status: "failed" })).toBeNull();
    expect(toRowGist({ ...gist("a"), gist: "  " })).toBeNull();
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

  test("an answer from before an invalidation can't put a line back", async () => {
    const held: { resolve?: (value: unknown) => void } = {};
    apiFetch.mockReturnValueOnce(
      new Promise((resolve) => {
        held.resolve = resolve;
      }),
    );
    const pending = requestRowGists(["a"], 1_000);
    // A new message lands in the conversation while the request is out.
    emit({
      type: "NewMessages",
      envelopes: [{ thread_id: "a" }],
      total: 1,
    } as unknown as DaemonEvent);
    held.resolve?.(answer([gist("a")]));
    await pending;
    expect(getRowGist("a")).toBeUndefined();
    // The row asks again at once, and this answer is kept.
    apiFetch.mockResolvedValue(answer([gist("a")]));
    await requestRowGists(["a"], 1_500);
    expect(getRowGist("a")).toBeDefined();
  });

  test("a sampled NewMessages drops every line", async () => {
    apiFetch.mockResolvedValue(answer([gist("a"), gist("b")]));
    await requestRowGists(["a", "b"], 1_000);
    emit({
      type: "NewMessages",
      envelopes: [{ thread_id: "z" }],
      total: 900,
    } as unknown as DaemonEvent);
    expect(getRowGist("a")).toBeUndefined();
    expect(getRowGist("b")).toBeUndefined();
  });

  test("lines and asked-about rows are capped, oldest first", async () => {
    for (let index = 0; index < STATE_CAP + 3; index += 1) putGist(gist(`t${index}`));
    expect(getRowGist("t0")).toBeUndefined();
    expect(getRowGist(`t${STATE_CAP + 2}`)).toBeDefined();
    resetRowGistsForTest();
    apiFetch.mockResolvedValue(answer([], "available"));
    const ids = Array.from({ length: STATE_CAP + 100 }, (_, index) => `r${index}`);
    // Sequential on purpose: the cap evicts the oldest asked-about rows, so
    // the batches must land in order.
    await ids.reduce<Promise<void>>(
      (chain, _id, index) =>
        index % 100 === 0
          ? chain.then(() => requestRowGists(ids.slice(index, index + 100), 1_000))
          : chain,
      Promise.resolve(),
    );
    apiFetch.mockClear();
    // The oldest asked-about rows were forgotten, so they are asked again.
    await requestRowGists(["r0", `r${STATE_CAP + 99}`], 2_000);
    expect(apiFetch.mock.calls[0]?.[1]).toMatchObject({ body: { thread_ids: ["r0"] } });
  });
  test("Updates asks for automated mail too, and a people-only ask earlier does not count", async () => {
    apiFetch.mockResolvedValue(answer([], "available"));
    // An inbox list asked about this thread a moment ago and the daemon
    // skipped it as not from a person.
    await requestRowGists(["a"], 1_000);
    await requestRowGists(["a"], 2_000, { automated: true });
    expect(apiFetch).toHaveBeenCalledTimes(2);
    expect(apiFetch.mock.calls[0]?.[1]).toEqual({
      method: "POST",
      body: { thread_ids: ["a"], generate: true },
    });
    expect(apiFetch.mock.calls[1]?.[1]).toEqual({
      method: "POST",
      body: { thread_ids: ["a"], generate: true, automated: true },
    });
    // Each kind of ask is remembered on its own.
    await requestRowGists(["a"], 3_000, { automated: true });
    expect(apiFetch).toHaveBeenCalledTimes(2);
  });

  test("a new message makes both kinds of ask possible again", async () => {
    apiFetch.mockResolvedValue(answer([], "available"));
    await requestRowGists(["a"], 1_000, { automated: true });
    emit({
      type: "NewMessages",
      envelopes: [{ thread_id: "a" }],
      total: 1,
    } as unknown as DaemonEvent);
    await requestRowGists(["a"], 1_500, { automated: true });
    expect(apiFetch).toHaveBeenCalledTimes(2);
  });

  test("odd-typed values and prototype keys are not words", () => {
    for (const odd of [{ toString: 1 }, ["x"], 5, true]) {
      expect(toRowGist({ ...gist("a"), gist: odd } as never)).toBeNull();
    }
    expect(
      toRowGist({ ...gist("a"), ask: { summary: { trim: 1 }, quote: null } } as never)?.ask,
    ).toBeNull();
    expect(toRowGist({ ...gist("a"), newest_message_id: { x: 1 } } as never)?.newest).toBeNull();
    // A thread called after an Object prototype member is just a thread.
    for (const id of ["__proto__", "constructor", "toString"]) {
      expect(getRowGist(id)).toBeUndefined();
      putGist(gist(id));
      expect(getRowGist(id)?.about).toContain("Canary");
    }
    expect(getRowGist("hasOwnProperty")).toBeUndefined();
  });
});

describe("row gists for a thread id held by two accounts", () => {
  const inAccount = (account: string, about: string) => ({
    ...gist("shared"),
    account_id: account,
    gist: about,
  });

  test("asks about each account's row and keeps their lines apart", async () => {
    apiFetch.mockResolvedValue(
      answer([inAccount("A", "A's line"), inAccount("B", "B's line")] as never),
    );
    await requestRowGists(
      [
        { thread_id: "shared", account_id: "A" },
        { thread_id: "shared", account_id: "B" },
      ],
      1_000,
    );
    expect(apiFetch.mock.calls[0]?.[1]).toMatchObject({
      body: {
        thread_ids: ["shared"],
        thread_accounts: [
          { thread_id: "shared", account_id: "A" },
          { thread_id: "shared", account_id: "B" },
        ],
      },
    });
    expect(getRowGist("shared", "A")?.about).toBe("A's line");
    expect(getRowGist("shared", "B")?.about).toBe("B's line");
  });

  test("new mail in A's thread drops A's line only", () => {
    putGist(inAccount("A", "A's line") as never);
    putGist(inAccount("B", "B's line") as never);
    forgetGists([{ thread_id: "shared", account_id: "A" }]);
    expect(getRowGist("shared", "A")).toBeUndefined();
    expect(getRowGist("shared", "B")?.about).toBe("B's line");
  });

  test("a NewMessages event names the account of each envelope", async () => {
    // Asking once starts the store listening to the daemon's events.
    apiFetch.mockResolvedValue(answer([]));
    await requestRowGists(["other"], 1_000);
    putGist(inAccount("A", "A's line") as never);
    putGist(inAccount("B", "B's line") as never);
    emit({
      type: "NewMessages",
      envelopes: [{ thread_id: "shared", account_id: "B" }],
      total: 1,
    } as unknown as DaemonEvent);
    expect(getRowGist("shared", "A")?.about).toBe("A's line");
    expect(getRowGist("shared", "B")).toBeUndefined();
  });
});
