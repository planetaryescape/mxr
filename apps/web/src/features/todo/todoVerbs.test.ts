import { afterEach, describe, expect, test, vi } from "vitest";
import { QueryClient, QueryObserver } from "@tanstack/react-query";

import { useUndo } from "@/state/undoStore";
import { setActiveQueryClient } from "@/lib/queryClient";
import * as modeDone from "@/features/modes/modeDone";

import { todoFixture } from "./testing";

const setCatchup = vi.fn<(...args: unknown[]) => Promise<unknown>>();
const setTodoState = vi.fn<(...args: unknown[]) => Promise<unknown>>();

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  setCatchup: (...args: unknown[]) => setCatchup(...args),
  setTodoState: (...args: unknown[]) => setTodoState(...args),
}));

const { decideCatchup, letGoAll, restoreTodos, tickOff, useTodoHidden } = await import("./todoVerbs");

function changed(ids: string[]) {
  return {
    dry_run: false,
    action: "x",
    summary: "",
    changed: ids.map((id) => todoFixture({ id })),
  };
}

afterEach(() => {
  setCatchup.mockReset();
  setTodoState.mockReset();
  useUndo.getState().clear();
  vi.restoreAllMocks();
  useTodoHidden.setState({ hidden: new Set(), leaving: new Set() });
});

test("duplicate tick-off cannot reveal the task before its original request settles", async () => {
  let finish!: (done: boolean) => void;
  const request = vi.spyOn(modeDone, "markModeDone")
    .mockImplementationOnce(() => new Promise<boolean>((resolve) => (finish = resolve)))
    .mockResolvedValue(false);
  const todo = todoFixture();
  const first = tickOff(todo);
  useTodoHidden.getState().hide([todo.id]); // The exit animation has finished.
  await tickOff(todo);
  const hiddenWhilePending = useTodoHidden.getState().hidden.has(todo.id);
  finish(true);
  await first;
  expect(request).toHaveBeenCalledTimes(1);
  expect(hiddenWhilePending).toBe(true);
  expect(useTodoHidden.getState().hidden.has(todo.id)).toBe(false);
});

describe("undoing a catch-up decision", () => {
  test.each(["keep", "let_go"] as const)(
    "after %s, u puts the row back in the batch undecided",
    async (decision) => {
      setCatchup.mockResolvedValue(changed(["todo_a"]));
      await decideCatchup("acct", [todoFixture({ id: "todo_a" })], decision);
      setCatchup.mockClear();
      const undo = useUndo.getState().lastUndo;
      expect(undo).toBeTypeOf("function");
      await undo!();
      expect(setCatchup).toHaveBeenCalledWith("acct", {
        decision: "undecide",
        todo_ids: ["todo_a"],
      });
      expect(setTodoState).not.toHaveBeenCalled();
    },
  );

  test("after letting go of all, u puts every previewed row back undecided", async () => {
    setCatchup.mockResolvedValue(changed(["todo_a", "todo_b"]));
    await letGoAll(null, [todoFixture({ id: "todo_a" }), todoFixture({ id: "todo_b" })]);
    expect(setCatchup).toHaveBeenCalledWith(null, {
      decision: "let_go",
      todo_ids: ["todo_a", "todo_b"],
    });
    setCatchup.mockClear();
    await useUndo.getState().lastUndo!();
    expect(setCatchup).toHaveBeenCalledWith(null, {
      decision: "undecide",
      todo_ids: ["todo_a", "todo_b"],
    });
  });
});

test("reopening a task refreshes its runway, Now and rail counts without waiting for polling", async () => {
  const qc = new QueryClient();
  setActiveQueryClient(qc);
  let serverCount = 5;
  const keys = [["rail", "all"], ["now", "all"], ["todos", "runway", "all"]];
  const observers = keys.map((queryKey) => {
    qc.setQueryData(queryKey, serverCount);
    return new QueryObserver(qc, {
      queryKey,
      queryFn: async () => serverCount,
      staleTime: Infinity,
    });
  });
  const unsubscribes = observers.map((observer) => observer.subscribe(() => {}));
  try {
    setTodoState.mockImplementationOnce(async () => {
      serverCount = 6;
      return changed(["reopened"]);
    });
    await restoreTodos([todoFixture({ id: "reopened", state: "done" })]);
    expect(observers.map((observer) => observer.getCurrentResult().data)).toEqual([6, 6, 6]);
  } finally {
    unsubscribes.forEach((unsubscribe) => unsubscribe());
    qc.clear();
  }
});
