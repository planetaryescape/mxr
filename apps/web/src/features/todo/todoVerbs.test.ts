import { afterEach, expect, test, vi } from "vitest";

import { useUndo } from "@/state/undoStore";
import * as modeDone from "@/features/modes/modeDone";

import { todoFixture } from "./testing";

const setCatchup = vi.fn<(...args: unknown[]) => Promise<unknown>>();
const setTodoState = vi.fn<(...args: unknown[]) => Promise<unknown>>();

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  setCatchup: (...args: unknown[]) => setCatchup(...args),
  setTodoState: (...args: unknown[]) => setTodoState(...args),
}));

const { tickOff, useTodoHidden } = await import("./todoVerbs");

afterEach(() => {
  setCatchup.mockReset();
  setTodoState.mockReset();
  useUndo.getState().clear();
  vi.restoreAllMocks();
  useTodoHidden.setState({ hidden: new Set(), leaving: new Set() });
});

test("duplicate tick-off cannot reveal the task before its original request settles", async () => {
  let finish!: (done: boolean) => void;
  const request = vi
    .spyOn(modeDone, "markModeDone")
    .mockImplementationOnce(() => new Promise<boolean>((resolve) => (finish = resolve)))
    .mockResolvedValue(false);
  const todo = todoFixture();
  const first = tickOff(todo);
  useTodoHidden.getState().hide([todo.id]);
  await tickOff(todo);
  const hiddenWhilePending = useTodoHidden.getState().hidden.has(todo.id);
  finish(true);
  await first;
  expect(request).toHaveBeenCalledTimes(1);
  expect(hiddenWhilePending).toBe(true);
  expect(useTodoHidden.getState().hidden.has(todo.id)).toBe(false);
});
