import { afterEach, describe, expect, test, vi } from "vitest";

import { useUndo } from "@/state/undoStore";

import { todoFixture } from "./testing";

const setCatchup = vi.fn<(...args: unknown[]) => Promise<unknown>>();
const setTodoState = vi.fn<(...args: unknown[]) => Promise<unknown>>();

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  setCatchup: (...args: unknown[]) => setCatchup(...args),
  setTodoState: (...args: unknown[]) => setTodoState(...args),
}));

const { decideCatchup, letGoAll } = await import("./todoVerbs");

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
