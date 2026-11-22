import { beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClient, QueryObserver } from "@tanstack/react-query";
import * as queryClient from "@/lib/queryClient";

import { resetLeaving } from "@/lib/leaving";
import { useUndo } from "@/state/undoStore";

import { doneToast, markModeDone, refreshModes, useModeDone, type ModeDoneOutcome } from "./modeDone";

const fetchMock = vi.hoisted(() => vi.fn<(path: string, opts?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", () => ({ apiFetch: fetchMock }));

const outcome = (thread: string, copy: string): ModeDoneOutcome => ({
  thread_id: thread,
  mode: "messages",
  provider: "Gmail",
  copy,
});

test("refreshing modes refetches the active To do runway once", async () => {
  const qc = new QueryClient();
  const activeClient = vi.spyOn(queryClient, "getActiveQueryClient").mockReturnValue(qc);
  const queryKey = ["todos", "runway", "all"];
  qc.setQueryData(queryKey, []);
  const fetchRunway = vi.fn<() => Promise<string[]>>(async () => ["new task"]);
  const observer = new QueryObserver(qc, { queryKey, queryFn: fetchRunway, staleTime: Infinity });
  const unsubscribe = observer.subscribe(() => {});
  try {
    await refreshModes();
    expect(fetchRunway).toHaveBeenCalledTimes(1);
    expect(observer.getCurrentResult().data).toEqual(["new task"]);
  } finally {
    unsubscribe();
    qc.clear();
    activeClient.mockRestore();
  }
});

const todoDone = (id: string) => ({
  kind: "ModeDone",
  dry_run: false,
  mutation_id: id,
  items: [{ thread_id: "shared", mode: "todo", provider: "Gmail", copy: "Ticked off." }],
});

describe("done here's toast", () => {
  test("one thread says where it went, in the daemon's words", () => {
    expect(doneToast([outcome("a", "Done in Messages. Still in To do (due Mon).")])).toBe(
      "Done in Messages. Still in To do (due Mon).",
    );
  });

  test("several that went the same way say how many and where", () => {
    expect(
      doneToast([
        outcome("a", "Done. Archived in Gmail."),
        outcome("b", "Done. Archived in Gmail."),
      ]),
    ).toBe("2 conversations: Done. Archived in Gmail.");
  });

});

describe("done here's fold follows how it was asked for", () => {
  const answer = {
    kind: "ModeDone",
    dry_run: false,
    items: [{ thread_id: "t3", mode: "messages", provider: "Gmail", copy: "Done." }],
  };

  test("a pointer folds the row while the request is out; nothing is left folding after", async () => {
    resetLeaving();
    let seen = false;
    fetchMock.mockImplementationOnce(() => {
      seen = useModeDone.getState().folding.messages.has("t3");
      return Promise.resolve(answer);
    });
    await markModeDone("messages", ["t3"], { via: "pointer" });
    expect(seen).toBe(true);
    expect(useModeDone.getState().folding.messages.has("t3")).toBe(false);
  });

  test("a key repeat drops the row without a fold", async () => {
    resetLeaving();
    fetchMock.mockResolvedValue(answer);
    let during = true;
    fetchMock.mockImplementationOnce(() => {
      during = useModeDone.getState().folding.messages.has("t3");
      return Promise.resolve(answer);
    });
    await markModeDone("messages", ["t3"], { via: "key" });
    fetchMock.mockImplementationOnce(() => {
      during = useModeDone.getState().folding.messages.has("t3");
      return Promise.resolve(answer);
    });
    await markModeDone("messages", ["t3"], { via: "key" });
    expect(during).toBe(false);
  });
});

describe("done here when nothing was done", () => {
  test("keeps the daemon's undo when it changed something part way", async () => {
    fetchMock.mockResolvedValueOnce({
      kind: "ModeDone",
      dry_run: false,
      mutation_id: "m-partial",
      items: [
        {
          thread_id: "t1",
          mode: "messages",
          provider: "Gmail",
          copy: "",
          error: "the mail server refused; run done again to retry",
        },
      ],
    });
    const done = await markModeDone("messages", ["t1"]);
    expect(done).toBe(false);
    expect(useUndo.getState().lastMutationId).toBe("m-partial");
    expect(useUndo.getState().lastUndo).not.toBeNull();
  });

});

describe("named To do completion in flight", () => {
  beforeEach(() => {
    fetchMock.mockReset();
    useUndo.getState().clear();
  });

  test("a duplicate activation sends once and preserves the successful Undo", async () => {
    let finish!: (value: ReturnType<typeof todoDone>) => void;
    fetchMock.mockImplementationOnce(() => new Promise<ReturnType<typeof todoDone>>((resolve) => (finish = resolve)));
    fetchMock.mockResolvedValueOnce({
      kind: "ModeDone",
      dry_run: false,
      items: [{ thread_id: "shared", mode: "todo", error: "no open to-do on this conversation" }],
    });
    const first = markModeDone("todo", ["shared"], { todoIds: ["one"] });
    await markModeDone("todo", ["shared"], { todoIds: ["one"] });
    finish(todoDone("undo-one"));
    await first;
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(useUndo.getState().lastMutationId).toBe("undo-one");
    expect(useUndo.getState().lastUndo).toBeTypeOf("function");
  });

  test("another task on the same email still completes while the first is pending", async () => {
    let finish!: (value: ReturnType<typeof todoDone>) => void;
    fetchMock.mockImplementationOnce(() => new Promise<ReturnType<typeof todoDone>>((resolve) => (finish = resolve)));
    fetchMock.mockResolvedValueOnce(todoDone("undo-two"));
    const first = markModeDone("todo", ["shared"], { todoIds: ["one"] });
    await markModeDone("todo", ["shared"], { todoIds: ["two"] });
    const hiddenWhileFirstPending = useModeDone.getState().hidden.todo.has("shared");
    finish(todoDone("undo-one"));
    await first;
    expect(hiddenWhileFirstPending).toBe(true);
    expect(useModeDone.getState().hidden.todo.has("shared")).toBe(false);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[1]?.[1]).toMatchObject({ body: { todo_ids: ["two"] } });
    expect(useUndo.getState().lastMutationId).toBe("undo-two");
  });

  test("a failed task can be retried after the request settles", async () => {
    fetchMock.mockRejectedValueOnce(new Error("offline"));
    await markModeDone("todo", ["shared"], { todoIds: ["one"] });
    fetchMock.mockResolvedValueOnce(todoDone("retry-undo"));
    expect(await markModeDone("todo", ["shared"], { todoIds: ["one"] })).toBe(true);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(useUndo.getState().lastMutationId).toBe("retry-undo");
  });

  test("an overlapping batch submits only tasks not already pending", async () => {
    let finish!: (value: ReturnType<typeof todoDone>) => void;
    fetchMock.mockImplementationOnce(() => new Promise<ReturnType<typeof todoDone>>((resolve) => (finish = resolve)));
    fetchMock.mockResolvedValueOnce(todoDone("undo-two"));
    const first = markModeDone("todo", ["shared"], { todoIds: ["one"] });
    await markModeDone("todo", ["shared"], { todoIds: ["one", "two", "two"] });
    finish(todoDone("undo-one"));
    await first;
    expect(fetchMock.mock.calls[1]?.[1]).toMatchObject({ body: { todo_ids: ["two"] } });
    expect(useUndo.getState().lastMutationId).toBe("undo-two");
  });
});
