import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { Todo } from "./api";
import { todoFixture } from "./testing";
import { TodoRow, type TodoRowProps } from "./TodoRow";
import type { Band } from "./todoRows";

function renderRow(todo: Todo, band: Band = "now", overrides: Partial<TodoRowProps> = {}) {
  const props: TodoRowProps = {
    todo,
    band,
    index: 0,
    focused: true,
    expanded: false,
    onSelect: vi.fn<(todo: Todo) => void>(),
    onPrimary: vi.fn<(todo: Todo) => void>(),
    onDone: vi.fn<(todo: Todo) => void>(),
    onRestore: vi.fn<(todo: Todo) => void>(),
    onToggleSource: vi.fn<(todo: Todo) => void>(),
    onOpenEmail: vi.fn<(todo: Todo) => void>(),
    ...overrides,
  };
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ul>
        <TodoRow {...props} />
      </ul>
    </QueryClientProvider>,
  );
  return { props, row: screen.getByTestId("todo-row") };
}

describe("TodoRow", () => {
  test("reads as an instruction: verb and object, who, amount, dates and one button", () => {
    const { row, props } = renderRow(todoFixture());
    const view = within(row);
    expect(view.getByTestId("todo-title")).toHaveTextContent("Pay council tax");
    expect(row).toHaveTextContent("Camden Council");
    expect(view.getByTestId("todo-amount")).toHaveTextContent("£142.00");
    expect(view.getByTestId("todo-when")).toHaveTextContent("act by Wed 7 Oct · due Fri 9 Oct");
    expect(view.getByTestId("runway-bar")).toHaveAttribute("data-fill", "0.60");
    expect(view.getByTestId("todo-why")).toHaveTextContent('"payment due 9 October"');
    const button = view.getByTestId("todo-action");
    expect(button).toHaveTextContent("Open email to pay");
    expect(view.getByTestId("todo-action-domain")).toHaveTextContent("camden.gov.uk");
    expect(
      view.getAllByRole("button").filter((b) => b.dataset.testid === "todo-action"),
    ).toHaveLength(1);
    fireEvent.click(button);
    expect(props.onPrimary).toHaveBeenCalledWith(props.todo);
  });

  test("a row about a link renders no outside link to click", () => {
    const { row } = renderRow(
      todoFixture({
        action: {
          label: "Pay on camden.gov.uk",
          url: "https://www.camden.gov.uk/pay-council-tax",
          domain: "camden.gov.uk",
          trusted: true,
        },
      }),
    );
    expect(row.querySelector("a[href]")).toBeNull();
    expect(within(row).getByTestId("todo-action")).toHaveAttribute("data-kind", "email");
  });

  test("overdue reads 'was due' with no bar and nothing in red", () => {
    const { row } = renderRow(
      todoFixture({ overdue: true, when_label: "was due Fri 2 Oct", runway: 1 }),
    );
    expect(within(row).getByTestId("todo-when")).toHaveTextContent("was due Fri 2 Oct");
    expect(within(row).queryByTestId("runway-bar")).toBeNull();
    expect(row.innerHTML).not.toMatch(/destructive|text-red|bg-red|warning/);
  });

  test("Coming up rows say when they show up, without a button", () => {
    const { row } = renderRow(
      todoFixture({ when_label: "shows up Mon 12 Oct · act by Thu 15 Oct", runway: 0 }),
      "coming",
    );
    expect(row).toHaveTextContent("shows up Mon 12 Oct · act by Thu 15 Oct");
    expect(within(row).queryByTestId("todo-action")).toBeNull();
  });

  test("the provenance chip shows each field's source and what to check", () => {
    const onToggleSource = vi.fn<(todo: Todo) => void>();
    const { row } = renderRow(todoFixture(), "now", { onToggleSource });
    const chip = within(row).getByTestId("todo-provenance");
    expect(chip).toHaveTextContent("from schema.org, 1 to check");
    expect(chip.getAttribute("title")).toContain("Due: a pattern in the email (check this)");
    fireEvent.click(chip);
    expect(onToggleSource).toHaveBeenCalled();
  });

  test("ticking off from the row's check", () => {
    const { row, props } = renderRow(todoFixture());
    fireEvent.click(within(row).getByTestId("todo-done"));
    expect(props.onDone).toHaveBeenCalledWith(props.todo);
  });
});
