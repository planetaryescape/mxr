import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import { TodoCorrectionDialog } from "./TodoCorrectionDialog";
import { todoFixture } from "./testing";
import { correctRow } from "./todoVerbs";

vi.mock("./todoVerbs", () => ({ correctRow: vi.fn() }));
afterEach(() => { vi.clearAllMocks(); useMailDialogs.getState().close(); });

test("wrong details opens the editor and keeps the source quote unchanged", () => {
  const todo = todoFixture({ why: "Please sign the original contract." });
  const close = vi.fn();
  render(<TodoCorrectionDialog todo={todo} onClose={close} />);
  expect(screen.getByText(todo.why)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Wrong details" }));
  expect(useMailDialogs.getState().dialog).toEqual({ kind: "todo-edit", todo });
  expect(correctRow).not.toHaveBeenCalled();
  expect(close).not.toHaveBeenCalled();
});

test.each([
  ["Not a to-do", "not_a_todo"], ["Not mine", "not_mine"], ["Already done", "already_done"],
] as const)("%s keeps the picker open when persistence fails", async (label, reason) => {
  vi.mocked(correctRow).mockResolvedValue(null);
  const todo = todoFixture(); const close = vi.fn();
  render(<TodoCorrectionDialog todo={todo} onClose={close} />);
  fireEvent.click(screen.getByRole("button", { name: label }));
  expect(correctRow).toHaveBeenCalledWith(todo, reason);
  await screen.findByRole("button", { name: "Wrong details" });
  expect(close).not.toHaveBeenCalled();
});
