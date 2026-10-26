import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, expect, test, vi } from "vitest";
import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import { useUndo } from "@/state/undoStore";
import { correctTodo, undoTodoCorrection } from "./api";
import { TodoCorrectionDialog } from "./TodoCorrectionDialog";
import { TodoEditDialog } from "./TodoDialogs";
import { todoFixture } from "./testing";

vi.mock("./api", async (original) => ({
  ...(await original<typeof import("./api")>()),
  correctTodo: vi.fn<typeof correctTodo>(),
  undoTodoCorrection: vi.fn<typeof undoTodoCorrection>(),
}));

const originalScrollIntoView = HTMLElement.prototype.scrollIntoView;
beforeAll(() => {
  HTMLElement.prototype.scrollIntoView = vi.fn<() => void>();
});
afterAll(() => {
  HTMLElement.prototype.scrollIntoView = originalScrollIntoView;
});

afterEach(() => {
  vi.clearAllMocks();
  useMailDialogs.getState().close();
  useUndo.getState().clear();
});

async function chooseSign() {
  fireEvent.keyDown(screen.getByRole("combobox", { name: "Kind" }), { key: "Enter" });
  fireEvent.click(await screen.findByRole("option", { name: "Sign" }));
}

test("Wrong details saves kind through the correction request and keeps exact Undo", async () => {
  const todo = todoFixture();
  const close = vi.fn<() => void>();
  const picker = render(<TodoCorrectionDialog todo={todo} onClose={close} />);
  fireEvent.click(screen.getByRole("button", { name: "Wrong details" }));
  expect(useMailDialogs.getState().dialog).toEqual({ kind: "todo-edit", todo });
  picker.unmount();
  render(<TodoEditDialog todo={todo} onClose={close} />);
  await chooseSign();
  vi.mocked(correctTodo).mockResolvedValue({
    dry_run: false,
    action: "wrong_details",
    summary: "Corrected",
    changed: [{ ...todo, kind: "sign" }],
    correction_undo: "saved-token",
  });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(close).toHaveBeenCalledOnce());
  expect(correctTodo).toHaveBeenCalledWith(todo.id, "wrong_details", [
    { field: "kind", value: "sign" },
  ]);
  expect(screen.getByText(todo.why)).toBeInTheDocument();
  await useUndo.getState().lastUndo?.();
  expect(undoTodoCorrection).toHaveBeenCalledWith(todo.id, "saved-token");
});

test("failed mixed-field correction retains every input and retries the same edits", async () => {
  const todo = todoFixture();
  const close = vi.fn<() => void>();
  render(<TodoEditDialog todo={todo} onClose={close} />);
  await chooseSign();
  fireEvent.change(screen.getByLabelText("What to do"), {
    target: { value: "Sign the payment plan" },
  });
  fireEvent.change(screen.getByLabelText("Who"), { target: { value: "Nora" } });
  fireEvent.change(screen.getByLabelText("Amount"), { target: { value: "£25" } });
  fireEvent.change(screen.getByLabelText("Due"), { target: { value: "tomorrow" } });
  vi.mocked(correctTodo).mockRejectedValueOnce(new Error("503 unavailable"));
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
  expect(close).not.toHaveBeenCalled();
  expect(screen.getByLabelText("What to do")).toHaveValue("Sign the payment plan");
  expect(screen.getByLabelText("Who")).toHaveValue("Nora");
  expect(screen.getByLabelText("Amount")).toHaveValue("£25");
  expect(screen.getByLabelText("Due")).toHaveValue("tomorrow");
  expect(screen.getByRole("combobox", { name: "Kind" })).toHaveTextContent("Sign");
  expect(screen.getByText(todo.why)).toBeInTheDocument();
  const first = vi.mocked(correctTodo).mock.calls[0];
  vi.mocked(correctTodo).mockResolvedValueOnce({
    dry_run: false,
    action: "wrong_details",
    summary: "Corrected",
    changed: [{ ...todo, kind: "sign" }],
  });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(close).toHaveBeenCalledOnce());
  expect(vi.mocked(correctTodo).mock.calls[1]).toEqual(first);
});

test("an acknowledgement for the original task cannot close a replacement editor", async () => {
  const todo = todoFixture();
  let finish: ((change: Awaited<ReturnType<typeof correctTodo>>) => void) | undefined;
  vi.mocked(correctTodo).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const close = vi.fn<() => void>();
  const view = render(<TodoEditDialog todo={todo} onClose={close} />);
  await chooseSign();
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  view.rerender(<TodoEditDialog todo={todoFixture({ id: "replacement" })} onClose={close} />);
  finish?.({
    dry_run: false,
    action: "wrong_details",
    summary: "Corrected",
    changed: [{ ...todo, kind: "sign" }],
  });
  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
  expect(close).not.toHaveBeenCalled();
});
