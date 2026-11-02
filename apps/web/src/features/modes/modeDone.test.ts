import { beforeEach, describe, expect, test, vi } from "vitest";

import { resetLeaving } from "@/lib/leaving";
import { useUndo } from "@/state/undoStore";

import { doneToast, markModeDone, useModeDone, type ModeDoneOutcome } from "./modeDone";

const fetchMock = vi.hoisted(() => vi.fn<(path: string, opts?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", () => ({ apiFetch: fetchMock }));

const outcome = (thread: string, copy: string): ModeDoneOutcome => ({
  thread_id: thread,
  mode: "messages",
  provider: "Gmail",
  copy,
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

  test("several that went different ways say how many", () => {
    expect(
      doneToast([
        outcome("a", "Done. Archived in Gmail."),
        outcome("b", "Done in Messages. Still in To do."),
      ]),
    ).toBe("Done with 2 conversations.");
    expect(doneToast([])).toBe("");
  });

  test("hides a row per mode, so done in one mode leaves the others", () => {
    const store = useModeDone.getState();
    store.hide("messages", ["t1"]);
    expect(useModeDone.getState().hidden.messages.has("t1")).toBe(true);
    expect(useModeDone.getState().hidden.todo.has("t1")).toBe(false);
    store.show("messages", ["t1"]);
    expect(useModeDone.getState().hidden.messages.has("t1")).toBe(false);
  });

  test("a row folds when the done asked for it, and drops out once its fold settles", () => {
    const store = useModeDone.getState();
    store.fold("messages", ["t2"]);
    expect(useModeDone.getState().folding.messages.has("t2")).toBe(true);
    expect(useModeDone.getState().folding.todo.has("t2")).toBe(false);
    store.settle("messages", ["t2"]);
    expect(useModeDone.getState().folding.messages.has("t2")).toBe(false);
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
    // The first key of a burst folds; the second, straight after, does not.
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

  test("names only the to-do being ticked off", async () => {
    fetchMock.mockResolvedValueOnce({
      kind: "ModeDone",
      dry_run: false,
      items: [{ thread_id: "t1", mode: "todo", provider: "Gmail", copy: "Ticked off." }],
    });
    await markModeDone("todo", ["t1"], { todoIds: ["todo-a"] });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/mail/modes/todo/done", {
      method: "POST",
      body: { thread_ids: ["t1"], dry_run: false, todo_ids: ["todo-a"] },
    });
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
    finish(todoDone("undo-one"));
    await first;
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
