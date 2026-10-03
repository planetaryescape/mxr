import { describe, expect, test, vi } from "vitest";

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
