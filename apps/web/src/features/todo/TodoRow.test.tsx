import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { Todo } from "./api";
import { todoFixture } from "./testing";
import { TodoRow, type TodoRowProps } from "./TodoRow";
import type { Band } from "./todoRows";
import { resetDaemonAvailabilityForTest } from "@/lib/daemonAvailability";
import { setToken } from "@/lib/tokenStorage";

const markRowSeen = vi.fn<(row: Todo) => void>();
vi.mock("@/features/modes/rowSeen", () => ({ useMarkRowSeen: () => markRowSeen }));

beforeEach(() => {
  resetDaemonAvailabilityForTest();
  markRowSeen.mockReset();
});
afterEach(() => vi.unstubAllGlobals());

function renderRow(todo: Todo, band: Band = "now", overrides: Partial<TodoRowProps> = {}) {
  const props: TodoRowProps = {
    todo,
    band,
    index: 0,
    lit: true,
    via: "key",
    details: false,
    onSelect: vi.fn<(todo: Todo) => void>(),
    onDone: vi.fn<(todo: Todo, via: "key" | "pointer") => void>(),
    onRestore: vi.fn<(todo: Todo) => void>(),
    onOpenEmail: vi.fn<(todo: Todo) => void>(),
    onToggleDetails: vi.fn<(todo: Todo) => void>(),
    onOpenResolution: vi.fn<(todo: Todo, threadId: string, messageId: string) => void>(),
    ...overrides,
  };
  const queryClient = new QueryClient();
  const view = render(
    <QueryClientProvider client={queryClient}>
      <ul>
        <TodoRow {...props} />
      </ul>
    </QueryClientProvider>,
  );
  return {
    props,
    row: screen.getByTestId("todo-row"),
    rerender: (next: Todo) =>
      view.rerender(
        <QueryClientProvider client={queryClient}>
          <ul>
            <TodoRow {...props} todo={next} />
          </ul>
        </QueryClientProvider>,
      ),
  };
}

describe("TodoRow", () => {
  test("e done and o email call their handlers", () => {
    const { row, props } = renderRow(todoFixture());
    fireEvent.click(within(row).getByTestId("todo-tick"));
    expect(props.onDone).toHaveBeenCalledWith(props.todo, "pointer");
    fireEvent.click(within(row).getByRole("button", { name: "email" }));
    expect(props.onOpenEmail).toHaveBeenCalledWith(props.todo);
    expect(props.onSelect).not.toHaveBeenCalled();
  });

  test("a late preview cannot authorize a different selected offer", async () => {
    let respond: ((response: Response) => void) | undefined;
    const response = new Promise<Response>((resolve) => {
      respond = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(() => response),
    );
    setToken("todo-preview-test");
    const makeTodo = (id: string, resolutionId: string): Todo =>
      todoFixture({
        id,
        thread_id: null,
        resolution: {
          id: resolutionId,
          message_id: `message-${id}`,
          thread_id: `thread-${id}`,
          source_date: "2026-10-04T09:00:00Z",
          quote: `Evidence for ${id}`,
          quote_location: "body",
          source_method: "model",
          action: "complete",
          proposed_due_at: null,
          proposed_due_words: null,
          status: "pending",
        },
      });
    const { rerender } = renderRow(makeTodo("todo-a", "offer-a"), "now", { details: true });
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    rerender(makeTodo("todo-b", "offer-b"));

    await act(async () => {
      respond?.(
        new Response(
          JSON.stringify({
            kind: "TodoChange",
            change: {
              action: "preview",
              changed: [],
              dry_run: true,
              summary: "Would complete offer A",
            },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        ),
      );
    });
    await waitFor(() => expect(screen.getByRole("button", { name: "Review" })).toBeEnabled());

    expect(screen.queryByTestId("todo-resolution-preview")).toBeNull();
    expect(screen.queryByRole("button", { name: "Accept and complete" })).toBeNull();
  });

  test("a completed row stays checked and reopens from the same checkbox", () => {
    const { row, props } = renderRow(todoFixture(), "done");
    const checkbox = within(row).getByRole("checkbox", { name: "Reopen Pay council tax" });

    expect(checkbox).toHaveAttribute("aria-checked", "true");
    fireEvent.click(checkbox);

    expect(props.onRestore).toHaveBeenCalledWith(props.todo);
    expect(props.onDone).not.toHaveBeenCalled();
    expect(props.onSelect).not.toHaveBeenCalled();
  });
});
