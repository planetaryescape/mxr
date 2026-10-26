import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { Todo } from "./api";
import { todoFixture } from "./testing";
import { TodoRow, type TodoRowProps } from "./TodoRow";
import type { Band } from "./todoRows";
import { resetDaemonAvailabilityForTest } from "@/lib/daemonAvailability";
import { setToken } from "@/lib/tokenStorage";

beforeEach(() => resetDaemonAvailabilityForTest());
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
  test("shows the task, who it is from and the window in words, with no bar or icon", () => {
    const { row } = renderRow(todoFixture(), "now", { lit: false });
    const view = within(row);
    expect(view.getByTestId("todo-title")).toHaveTextContent("Pay council tax");
    expect(row).toHaveTextContent("Camden Council");
    expect(row).toHaveTextContent(/from \w+|from today|from yesterday/);
    expect(view.getByTestId("todo-amount")).toHaveTextContent("£142.00");
    expect(view.getByTestId("todo-when")).toHaveTextContent("act by Wed 7 Oct · due Fri 9 Oct");
    expect(view.queryByTestId("runway-bar")).toBeNull();
    expect(view.queryByTestId("todo-action")).toBeNull();
    expect(view.queryByTestId("todo-provenance")).toBeNull();
    expect(row.querySelector("svg")).toBeNull();
  });

  test("a field to check shows as one word in the trail, not a chip", () => {
    const { row } = renderRow(todoFixture(), "now", { lit: false });
    expect(within(row).getByTestId("todo-unchecked")).toHaveTextContent("unchecked");
    expect(within(row).getByTestId("todo-unchecked").getAttribute("title")).toContain(
      "Due: a pattern in the email",
    );
  });

  test("the lit row offers e done and o email; the unlit row's verbs wait for hover", () => {
    const lit = renderRow(todoFixture(), "now", { lit: true });
    expect(lit.row).toHaveAttribute("data-lit", "true");
    expect(within(lit.row).getByTestId("todo-tick")).toBeVisible();
    expect(within(lit.row).getByRole("button", { name: "email" })).toBeVisible();
  });

  test("e done and o email call their handlers", () => {
    const { row, props } = renderRow(todoFixture());
    fireEvent.click(within(row).getByTestId("todo-tick"));
    expect(props.onDone).toHaveBeenCalledWith(props.todo, "pointer");
    fireEvent.click(within(row).getByRole("button", { name: "email" }));
    expect(props.onOpenEmail).toHaveBeenCalledWith(props.todo);
    expect(props.onSelect).not.toHaveBeenCalled();
  });

  test("a row with no email offers no o email", () => {
    const { row } = renderRow(todoFixture({ thread_id: null }));
    expect(within(row).queryByRole("button", { name: "email" })).toBeNull();
  });

  test("a row's date takes the cue colour only when it is due today or past", () => {
    const past = renderRow(todoFixture({ overdue: true, when_label: "was due Fri 2 Oct" }));
    expect(within(past.row).getByTestId("todo-when").className).toContain("text-cue");
    past.row.remove();
    const later = renderRow(
      todoFixture({
        overdue: false,
        due_at: new Date(Date.now() + 5 * 86_400_000).toISOString(),
        act_by_at: null,
      }),
    );
    expect(within(later.row).getByTestId("todo-when").className).not.toContain("text-cue");
  });

  test("a later-mail resolution shows its evidence and opens that exact source", () => {
    const onOpenResolution = vi.fn<(todo: Todo, threadId: string, messageId: string) => void>();
    const todo = todoFixture({
      resolution: {
        id: "resolution-1",
        message_id: "msg_resolution",
        thread_id: "thread_bill",
        source_date: "2026-10-04T09:00:00Z",
        quote: "Your council tax payment has been received.",
        quote_location: "body",
        source_method: "rule",
        action: "complete",
        proposed_due_at: null,
        proposed_due_words: null,
        status: "pending",
      },
    });
    const { row } = renderRow(todo, "now", { onOpenResolution, details: true });
    const card = within(row).getByTestId("todo-resolution");
    expect(card).toHaveTextContent("Your council tax payment has been received.");
    expect(card.querySelector("p")).toHaveClass("line-clamp-2");
    fireEvent.click(within(card).getByRole("button", { name: "Source email" }));
    expect(onOpenResolution).toHaveBeenCalledWith(todo, "thread_bill", "msg_resolution");
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
        // No source email: the one stubbed fetch is the preview's.
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

  test("expanded details preserve arrival, why, next and per-field unchecked words", () => {
    const { row } = renderRow(
      todoFixture({ next: "Check the council tax account", thread_id: null }),
      "now",
      { details: true },
    );
    const details = within(row).getByTestId("todo-source");

    expect(within(details).getByTestId("todo-arrived")).toHaveAttribute(
      "dateTime",
      "2026-10-02T09:00:00Z",
    );
    expect(within(details).getByTestId("todo-why")).toHaveTextContent('"payment due 9 October"');
    expect(details).toHaveTextContent("Check the council tax account");
    expect(details).toHaveTextContent("09/10/2026");
    expect(within(details).getAllByTestId("todo-field-unchecked")).toHaveLength(1);
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
  });

  test("overdue reads 'was due' and nothing is red", () => {
    const { row } = renderRow(
      todoFixture({ overdue: true, when_label: "was due Fri 2 Oct", runway: 1 }),
    );
    expect(within(row).getByTestId("todo-when")).toHaveTextContent("was due Fri 2 Oct");
    expect(row.innerHTML).not.toMatch(
      /(?<![\w-])(?<!aria-invalid:)(text|bg|border)-(destructive|red|warning)/,
    );
  });

  test("Coming up rows say when they show up", () => {
    const { row } = renderRow(
      todoFixture({ when_label: "shows up Mon 12 Oct · act by Thu 15 Oct", runway: 0 }),
      "coming",
    );
    expect(row).toHaveTextContent("shows up Mon 12 Oct · act by Thu 15 Oct");
  });

  test("keeps a visible checkbox before the title and does not select the row", () => {
    const { row, props } = renderRow(todoFixture());
    const checkbox = within(row).getByRole("checkbox", { name: "Tick off Pay council tax" });
    expect(checkbox).toHaveAttribute("data-testid", "todo-done");
    expect(checkbox).toHaveAttribute("aria-checked", "false");
    expect(row.querySelector("[data-testid='todo-title']")?.compareDocumentPosition(checkbox)).toBe(
      Node.DOCUMENT_POSITION_PRECEDING,
    );

    fireEvent.click(checkbox);
    expect(props.onDone).toHaveBeenCalledWith(props.todo, "pointer");
    expect(props.onSelect).not.toHaveBeenCalled();
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

  test("coming rows also expose the checkbox before their title", () => {
    const { row, props } = renderRow(todoFixture(), "coming");
    const checkbox = within(row).getByRole("checkbox", { name: "Tick off Pay council tax" });

    expect(checkbox).toHaveAttribute("aria-checked", "false");
    fireEvent.click(checkbox);
    expect(props.onDone).toHaveBeenCalledWith(props.todo, "pointer");
  });
});

test("the visible row keeps requester and the exact ask after title edits", () => {
  const quote = "Please sign the original contract.";
  const { row } = renderRow(
    todoFixture({
      title: "Sign the corrected contract",
      why: quote,
      person_label: "Alex asked",
      fields: [
        {
          field: "quote",
          evidence: quote,
          source: "rule",
          source_label: "a pattern in the email",
          checked: true,
        },
      ],
    }),
  );
  expect(row).toHaveTextContent("Alex asked");
  expect(within(row).getByTestId("todo-ask-quote")).toHaveTextContent(quote);
  expect(within(row).getByTestId("todo-title")).toHaveTextContent("Sign the corrected contract");
});

test("source evidence and the phone window share one secondary line with full evidence available", () => {
  const quote =
    "Please sign the original agreement,\nincluding the unchanged payment schedule by Friday.";
  const todo = todoFixture({
    title: "Sign the corrected agreement",
    person_label: "Alex asked",
    thread_id: null,
    fields: [
      {
        field: "quote",
        evidence: quote,
        source: "rule",
        source_label: "a pattern in the email",
        checked: true,
      },
    ],
  });
  const { row } = renderRow(todo, "now", { details: true });
  const evidence = within(row).getByTestId("todo-ask-quote");
  expect(evidence).not.toHaveClass("block");
  expect(evidence.textContent).toBe(`“${quote}”`);
  expect(evidence).toHaveAccessibleDescription(quote);
  const summary = within(row).getByTestId("todo-request-and-quote");
  expect(summary).toHaveTextContent("Alex asked");
  expect(summary).toContainElement(evidence);
  expect(summary).toHaveClass("min-w-0", "flex-1", "truncate");
  const secondary = within(row).getByTestId("todo-secondary");
  expect(secondary).toHaveClass("flex", "items-baseline");
  expect(secondary.querySelector("br")).toBeNull();
  const phoneWindow = within(secondary).getByTestId("todo-mobile-when");
  expect(phoneWindow).toHaveTextContent(todo.when_label);
  expect(phoneWindow).toHaveClass("shrink-0", "truncate", "sm:hidden");
  expect(within(row).getByRole("checkbox", { name: `Tick off ${todo.title}` })).toBeEnabled();
  const details = within(row).getByTestId("todo-source");
  expect(details).toHaveTextContent("including the unchanged payment schedule by Friday.");
});
