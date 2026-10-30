import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { putGist, resetRowGistsForTest } from "@/features/gists/rowGists";
import { formatReceived, formatWhen } from "@/lib/format";

import type { UpdateLine } from "./api";
import { lineFixture } from "./testing";
import { UpdateLineRow } from "./UpdateLineRow";

const apiFetch = vi.fn<(path: string) => Promise<unknown>>();
vi.mock("@/api/client", () => ({ apiFetch: (path: string) => apiFetch(path) }));

beforeEach(() => {
  apiFetch.mockReset();
  // The peek's preview reads the thread; most tests have no body to show.
  apiFetch.mockResolvedValue({ thread: {}, messages: [], bodies: [] });
  resetRowGistsForTest();
});

function makeHandlers() {
  return {
    onSelect: vi.fn<(line: UpdateLine) => void>(),
    onLetGo: vi.fn<(line: UpdateLine) => void>(),
    onNeedsMe: vi.fn<(line: UpdateLine) => void>(),
    onTune: vi.fn<(line: UpdateLine) => void>(),
    onOpenEmail: vi.fn<(line: UpdateLine) => void>(),
    onTuneTo: vi.fn<(line: UpdateLine, setting: "muted" | "changes_only") => void>(),
    onSweep: vi.fn<(line: UpdateLine) => void>(),
    onPin: vi.fn<(line: UpdateLine) => void>(),
    onTogglePeek: vi.fn<(line: UpdateLine) => void>(),
  };
}

function renderLine(line: UpdateLine, options: { focused?: boolean; peeked?: boolean } = {}) {
  const h = makeHandlers();
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <ul>
        <UpdateLineRow
          line={line}
          index={0}
          lit={options.focused ?? true}
          via="key"
          peeked={options.peeked ?? false}
          {...h}
        />
      </ul>
    </QueryClientProvider>,
  );
  return { handlers: h, row: screen.getByTestId("update-line") };
}

function openMenu(row: HTMLElement, name: string) {
  fireEvent.pointerDown(within(row).getByRole("button", { name }), { button: 0, ctrlKey: false });
}

describe("UpdateLineRow at rest", () => {
  test("a row is its source, its fact and one trailing value, with no buttons in the way", () => {
    const { row } = renderLine(lineFixture(), { focused: false });
    const view = within(row);
    expect(view.getByTestId("update-source")).toHaveTextContent("Strava");
    expect(view.getByTestId("update-fact")).toHaveTextContent(
      "Your week in running: 21.3 km over 3 runs",
    );
    expect(view.getByTestId("update-delta")).toHaveTextContent("up 12% on last week");
    expect(view.getByTestId("update-delta")).toHaveAttribute(
      "title",
      "Computed from 21.3 km and 19.0 km",
    );
    // A resting row's verbs wait for the lamp or a hover (Row hides them in CSS); the menu is among them.
    expect(row).not.toHaveAttribute("data-lit");
    expect(view.getByRole("button", { name: "More actions for Strava" })).toBeInTheDocument();
    expect(view.queryByText("Why this update?")).toBeNull();
    expect(view.queryByRole("link")).toBeNull();
    expect(view.queryByTestId("update-peek")).toBeNull();
  });

  test("the fact keeps the full subject for a hover, since the row is one line", () => {
    const { row } = renderLine(lineFixture(), { focused: false });
    expect(within(row).getByTestId("update-fact")).toHaveAttribute("title", lineFixture().fact);
  });

  test("the delta is neutral, not cyan", () => {
    const { row } = renderLine(lineFixture());
    expect(within(row).getByTestId("update-delta")).not.toHaveClass("text-primary", "text-signal");
  });

  test("the trail's time is the latest email's arrival, lit or not; the peek gives it in full", () => {
    const line = lineFixture();
    const { row } = renderLine(line, { focused: false });
    expect(within(row).getByText(formatWhen(line.latest_at))).toBeInTheDocument();
    cleanup();
    const lit = renderLine(line, { focused: true, peeked: true });
    expect(within(lit.row).getByText(formatWhen(line.latest_at))).toBeInTheDocument();
    expect(within(lit.row).getByTestId("update-arrived")).toHaveTextContent(
      formatReceived(line.latest_at),
    );
  });

  test("a clock time is not shown at rest", () => {
    const { row } = renderLine(
      lineFixture({
        section: "needs_a_look",
        signal: "needs_you",
        fact: "Security alert: New sign-in",
        delta: undefined,
        link: undefined,
        time_label: "06:12",
      }),
    );
    expect(row).not.toHaveTextContent("06:12");
  });

  test("a routine count carries its unit, and a single message shows none", () => {
    const { row } = renderLine(
      lineFixture({ section: "routine", delta: undefined, numbers: [], count: 5 }),
    );
    expect(within(row).getByTestId("update-count")).toHaveTextContent(/^5 emails$/);
    cleanup();
    const single = renderLine(
      lineFixture({ section: "routine", delta: undefined, numbers: [], count: 1 }),
    );
    expect(within(single.row).queryByTestId("update-count")).toBeNull();
  });

  test("an amount the fact leaves out trails it; one the fact already says does not", () => {
    const amounts = [{ raw: "£33.33", value: 33.33, unit: "GBP" }];
    const { row } = renderLine(
      lineFixture({ fact: "Your scheduled payment", delta: undefined, numbers: amounts }),
    );
    expect(within(row).getByTestId("update-amount")).toHaveTextContent("£33.33");
    cleanup();
    const said = renderLine(
      lineFixture({ fact: "Paid £33.33 today", delta: undefined, numbers: amounts }),
    );
    expect(within(said.row).queryByTestId("update-amount")).toBeNull();
  });

  test("a wrong-shaped payload renders without throwing", () => {
    const { row } = renderLine(
      lineFixture({
        numbers: undefined,
        delta: undefined,
        tracker: {
          kind: "parcel",
          state: "x",
          state_label: "x",
          outcome: "progress",
          steps: undefined,
          step: null,
        },
      }),
    );
    expect(within(row).getByTestId("update-fact")).toHaveTextContent("Your week in running");
    expect(within(row).queryByTestId("parcel-track")).toBeNull();
  });
});

describe("UpdateLineRow's trailing slot", () => {
  test("a sign-in already in To do says so and opens no link", () => {
    const { row } = renderLine(
      lineFixture({
        section: "needs_a_look",
        signal: "needs_you",
        source_name: "Google",
        fact: "Security alert: New sign-in",
        delta: undefined,
        link: { url: "https://myaccount.google.com/", domain: "google.com" },
        in_todo: "already in To do",
        todo_id: "todo_1",
      }),
    );
    const view = within(row);
    expect(view.getByTestId("update-in-todo")).toHaveTextContent("already in To do");
    expect(view.queryByRole("link")).toBeNull();
    expect(view.queryByTestId("update-todo-suggestion")).toBeNull();
    openMenu(row, "More actions for Google");
    expect(screen.queryByRole("menuitem", { name: /This needs me|Add to To do/ })).toBeNull();
    expect(screen.queryByRole("menuitem", { name: /google\.com/ })).toBeNull();
  });

  test("a suggested to-do is one compact affordance that adds the to-do", () => {
    const { row, handlers: h } = renderLine(
      lineFixture({
        section: "needs_a_look",
        signal: "needs_you",
        todo_suggestion: "Suggested to-do: failed payment (rule). t adds it.",
        todo_title: "Check failed payment from Stripe",
        delta: undefined,
        latest_message_pinned: true,
      }),
    );
    const view = within(row);
    const affordance = view.getByTestId("update-todo-suggestion");
    expect(affordance).toHaveAccessibleName("Add to To do: Check failed payment from Stripe");
    expect(view.queryByRole("button", { name: "This needs me" })).toBeNull();
    fireEvent.click(affordance);
    expect(h.onNeedsMe).toHaveBeenCalledTimes(1);
  });

  test("the to-do affordance replaces the other trailing values", () => {
    const { row } = renderLine(
      lineFixture({
        section: "needs_a_look",
        todo_suggestion: "Suggested to-do: x",
        todo_title: "x",
        numbers: [{ raw: "21.3 km", value: 21.3, unit: "km" }],
        fact: "Something else",
      }),
    );
    expect(within(row).getByTestId("update-todo-suggestion")).toBeInTheDocument();
    expect(within(row).queryByTestId("update-delta")).toBeNull();
    expect(within(row).queryByTestId("update-amount")).toBeNull();
  });
});

describe("UpdateLineRow's source", () => {
  test("a Needs a look name keeps its sending host, muted, and the name stays whole", () => {
    const { row } = renderLine(
      lineFixture({
        section: "needs_a_look",
        source_name: "Google (accounts.google.com)",
        sender_email: "no-reply@accounts.google.com",
        delta: undefined,
      }),
    );
    const source = within(row).getByTestId("update-source");
    expect(source).toHaveTextContent("Google (accounts.google.com)");
    expect(within(row).getByTestId("update-host")).toHaveClass("type-meta", "text-text-3");
    expect(source).toHaveClass("font-semibold", "text-text-1");
  });

  test("a changed source is regular weight and carries no host", () => {
    const { row } = renderLine(lineFixture({ source_name: "Strava" }));
    expect(within(row).getByTestId("update-source")).not.toHaveClass("font-semibold");
    expect(within(row).queryByTestId("update-host")).toBeNull();
  });

  test("a routine source calms to the second text tier", () => {
    const { row } = renderLine(lineFixture({ section: "routine", source_name: "Strava" }));
    expect(within(row).getByTestId("update-source")).toHaveClass("text-text-2");
    expect(within(row).getByTestId("update-source")).not.toHaveClass("font-semibold");
  });
});

describe("UpdateLineRow's track", () => {
  const parcel = (overrides: Partial<UpdateLine> = {}) =>
    lineFixture({
      source_key: "parcel:1",
      source_name: "Bookshop",
      fact: "Parcel on its way",
      // The daemon's own generic parcel line is a rule fact, not a subject.
      fact_source: "rule",
      message_ids: [],
      latest_message_id: undefined,
      delta: undefined,
      link: undefined,
      tracker: {
        kind: "parcel",
        state: "out_for_delivery",
        state_label: "out for delivery",
        outcome: "progress",
        steps: ["ordered", "shipped", "out for delivery", "delivered"],
        step: 1,
        detail: "Expected by Thu 8 Oct · DHL",
        items: [
          { name: "Blue notebook", quantity: 1 },
          { name: "Pen set", quantity: 2 },
        ],
        tracking_number: "A-VERY-LONG-TRACKING-REFERENCE-1234567890",
        order_number: "ORDER-987",
      },
      ...overrides,
    });

  test("a parcel's track stands in the fact column for its generic fact; the fact waits in the peek", () => {
    const { row } = renderLine(parcel(), { focused: false });
    expect(within(row).getByTestId("update-fact")).toHaveTextContent(
      "Shipped · next: out for delivery",
    );
    expect(within(row).queryByText("Parcel on its way")).toBeNull();
    // The references and the items wait for the peek.
    expect(within(row).queryByTestId("parcel-identity")).toBeNull();
    expect(row).not.toHaveTextContent("A-VERY-LONG-TRACKING-REFERENCE");
  });

  test("a subject-derived parcel fact shows as written, with the track after it", () => {
    const { row } = renderLine(parcel({ fact: "Delayed to Fri 17 Oct", fact_source: "subject" }), {
      focused: false,
    });
    const fact = within(row).getByTestId("update-fact");
    expect(fact).toHaveTextContent("Delayed to Fri 17 Oct");
    expect(fact).toHaveTextContent("Shipped · next: out for delivery");
    expect(within(fact).getByText("Shipped · next: out for delivery")).toHaveClass("text-text-3");
    expect(within(row).queryByTestId("update-fact-full")).toBeNull();
  });

  test("a parcel in Needs a look shows its fact even when the fact is generic", () => {
    const { row } = renderLine(parcel({ section: "needs_a_look", signal: "needs_you" }), {
      focused: false,
    });
    expect(within(row).getByTestId("update-fact")).toHaveTextContent("Parcel on its way");
    expect(within(row).getByTestId("update-fact")).toHaveTextContent(
      "Shipped · next: out for delivery",
    );
  });

  test("a parcel that went quiet keeps the fact that says so", () => {
    const quiet = parcel({ fact: "Parcel from Bookshop went quiet" });
    const { row } = renderLine(
      { ...quiet, tracker: { ...quiet.tracker!, outcome: "quiet" } },
      {
        focused: false,
      },
    );
    expect(within(row).getByTestId("update-fact")).toHaveTextContent(
      "Parcel from Bookshop went quiet",
    );
  });

  test("the peek keeps the full fact when the track stands in for it", () => {
    const { row } = renderLine(parcel(), { focused: false, peeked: true });
    expect(within(row).getByTestId("update-fact-full")).toHaveTextContent("Parcel on its way");
  });

  test("a delivered parcel keeps its own fact, since there is no track left", () => {
    const { row } = renderLine(
      parcel({
        fact: "Parcel delivered",
        tracker: { ...parcel().tracker!, state: "delivered", step: 3 },
      }),
      { focused: false },
    );
    expect(within(row).getByTestId("update-fact")).toHaveTextContent("Parcel delivered");
  });

  test("a parcel's references and items are in its peek", () => {
    const { row } = renderLine(parcel(), { peeked: true });
    expect(within(row).getByTestId("parcel-identity")).toHaveTextContent(
      "Tracking reference: A-VERY-LONG-TRACKING-REFERENCE-1234567890 · Order reference: ORDER-987",
    );
    expect(within(row).getByRole("list", { name: "Available parcel items" })).toHaveTextContent(
      "Blue notebook",
    );
    expect(within(row).getByRole("list", { name: "Available parcel items" })).toHaveTextContent(
      "Pen set · quantity 2",
    );
    expect(within(row).getByText("Expected by Thu 8 Oct · DHL")).toBeInTheDocument();
    expect(within(row).getByTestId("update-fact-full")).toHaveTextContent("Parcel on its way");
  });

  test("a parcel has no sender's mail to sweep, pin or let go", () => {
    const { row } = renderLine(parcel());
    openMenu(row, "More actions for Bookshop");
    expect(screen.queryByRole("menuitem", { name: "Sweep sender" })).toBeNull();
    expect(screen.queryByRole("menuitem", { name: "Pin" })).toBeNull();
    expect(screen.queryByRole("menuitem", { name: "Let go" })).toBeNull();
    expect(screen.queryByRole("menuitem", { name: "Tune…" })).toBeNull();
  });
});

describe("UpdateLineRow's keys and focus", () => {
  test("the lit row names its verbs with their keys; the unlit row shows none by itself", () => {
    const { row } = renderLine(lineFixture(), { focused: true });
    expect(row).toHaveAttribute("data-lit", "true");
    const view = within(row);
    expect(view.getByRole("button", { name: "peek" })).toBeInTheDocument();
    expect(view.getByRole("button", { name: "let go" })).toBeInTheDocument();
    expect(view.getByRole("button", { name: "email" })).toBeInTheDocument();
    expect(view.queryByTestId("update-keys")).toBeNull();
  });
});

describe("UpdateLineRow's more menu", () => {
  test("it holds every verb the row has, and gates them as before", () => {
    const { row } = renderLine(
      lineFixture({
        suggestion: "You've let go of Strava 8 digests in a row without opening it.",
      }),
    );
    openMenu(row, "More actions for Strava");
    for (const name of [
      "This needs me",
      "Open strava.com",
      "Open email",
      "Pin",
      "Sweep sender",
      "Let go",
      "Tune…",
      "Mute",
      "Changes only",
    ]) {
      expect(screen.getByRole("menuitem", { name })).toBeInTheDocument();
    }
  });

  test("a suggested to-do reads Add to To do in the menu", () => {
    const { row } = renderLine(
      lineFixture({
        todo_suggestion: "Suggested to-do: x",
        todo_title: "x",
        section: "needs_a_look",
      }),
    );
    openMenu(row, "More actions for Strava");
    expect(screen.getByRole("menuitem", { name: "Add to To do" })).toBeInTheDocument();
  });

  test("a link that needs you is not in the menu", () => {
    const { row } = renderLine(
      lineFixture({
        section: "needs_a_look",
        signal: "needs_you",
        link: { url: "https://myaccount.google.com/", domain: "google.com" },
        delta: undefined,
      }),
    );
    openMenu(row, "More actions for Strava");
    expect(screen.queryByRole("menuitem", { name: "Open google.com" })).toBeNull();
  });

  test("closing the menu returns focus to the row, not its More trigger", async () => {
    const { row } = renderLine(lineFixture(), { focused: false });
    openMenu(row, "More actions for Strava");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(row).toHaveFocus();
  });

  test("an action that opens a dialog leaves focus to the dialog", async () => {
    const { row, handlers: h } = renderLine(lineFixture(), { focused: false });
    openMenu(row, "More actions for Strava");
    fireEvent.click(screen.getByRole("menuitem", { name: "Let go" }));
    expect(h.onLetGo).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(row).not.toHaveFocus();
  });

  test("each action acts on the line its row was rendered with", () => {
    const first = lineFixture({ id: "a", source_key: "a.example", source_name: "Alpha" });
    const second = lineFixture({ id: "b", source_key: "b.example", source_name: "Beta" });
    const h = makeHandlers();
    render(
      <ul>
        {[first, second].map((line, index) => (
          <UpdateLineRow
            key={line.id}
            line={line}
            index={index}
            lit={false}
            via="key"
            peeked={false}
            {...h}
          />
        ))}
      </ul>,
    );
    const rows = screen.getAllByTestId("update-line");
    openMenu(rows[1]!, "More actions for Beta");
    fireEvent.click(screen.getByRole("menuitem", { name: "Let go" }));
    expect(h.onLetGo).toHaveBeenCalledWith(expect.objectContaining({ id: "b" }));
    expect(h.onLetGo).toHaveBeenCalledTimes(1);
  });
});

describe("UpdateLineRow's peek", () => {
  test("it holds the why, the numbers, the suggestion with its answers, and the links", () => {
    const { row, handlers: h } = renderLine(
      lineFixture({
        time_label: "06:12",
        numbers: [{ raw: "21.3 km", value: 21.3, unit: "km" }],
        suggestion: "You've let go of Strava 8 digests in a row. Mute it, or changes only?",
      }),
      { peeked: true },
    );
    const peek = within(row).getByRole("region", { name: "Details for Strava" });
    expect(within(peek).getByTestId("update-why")).toHaveTextContent("In the 08:00 digest.");
    expect(within(peek).getByTestId("update-numbers")).toHaveTextContent("Numbers: 21.3 km");
    expect(peek).toHaveTextContent("At 06:12");
    fireEvent.click(within(peek).getByRole("button", { name: "Changes only" }));
    expect(h.onTuneTo).toHaveBeenCalledWith(expect.anything(), "changes_only");
    fireEvent.click(within(peek).getByRole("button", { name: "Mute" }));
    expect(h.onTuneTo).toHaveBeenCalledWith(expect.anything(), "muted");
    expect(within(peek).getByRole("link", { name: "Open strava.com" })).toHaveAttribute(
      "href",
      "https://www.strava.com/athlete/training",
    );
    fireEvent.click(within(peek).getByRole("button", { name: "Open email" }));
    expect(h.onOpenEmail).toHaveBeenCalledTimes(1);
  });

  test("a line that folds several emails says how many in its peek", () => {
    const { row } = renderLine(lineFixture({ section: "needs_a_look", count: 2 }), {
      peeked: true,
    });
    expect(within(row).getByTestId("update-count-folded")).toHaveTextContent(
      "2 emails in this digest",
    );
  });

  test("a line that needs you opens no link in its peek", () => {
    const { row } = renderLine(
      lineFixture({
        section: "needs_a_look",
        signal: "needs_you",
        link: { url: "https://myaccount.google.com/", domain: "google.com" },
        delta: undefined,
      }),
      { peeked: true },
    );
    const peek = within(row).getByRole("region", { name: "Details for Strava" });
    expect(within(peek).queryByRole("link")).toBeNull();
    expect(within(peek).getByRole("button", { name: "Open email" })).toBeInTheDocument();
  });

  test("clicking the row body toggles the peek; its controls and text do not", () => {
    const { row, handlers: h } = renderLine(
      lineFixture({
        todo_suggestion: "Suggested to-do: x",
        todo_title: "x",
        section: "needs_a_look",
      }),
      { focused: false, peeked: true },
    );
    fireEvent.click(within(row).getByTestId("update-fact"));
    expect(h.onTogglePeek).toHaveBeenCalledTimes(1);
    fireEvent.click(within(row).getByTestId("update-todo-suggestion"));
    expect(h.onTogglePeek).toHaveBeenCalledTimes(1);
    fireEvent.click(within(row).getByTestId("update-why"));
    expect(h.onTogglePeek).toHaveBeenCalledTimes(1);
    fireEvent.click(within(row).getByRole("region", { name: "Details for Strava" }));
    expect(h.onTogglePeek).toHaveBeenCalledTimes(1);
  });

  test("a long fact is whole in the peek, since the row truncates it; a short one is not repeated", () => {
    const long =
      "Your week in running: 21.3 km over 3 runs, your longest this month, and a new best";
    const { row } = renderLine(lineFixture({ fact: long }), { focused: false, peeked: true });
    expect(within(row).getByTestId("update-fact-full")).toHaveTextContent(long);
    cleanup();
    const short = renderLine(lineFixture(), { focused: false, peeked: true });
    expect(within(short.row).queryByTestId("update-fact-full")).toBeNull();
  });
});

function receiptGist(overrides: Record<string, unknown> = {}) {
  return {
    thread_id: "t1",
    status: "ready" as const,
    gist: "Receipt: $24.00 for Perplexity Pro, renews 14 Nov",
    ask: null,
    provenance: { model: "gemma4", locality: "local" as const, sources: ["this_thread" as const] },
    from_cache: false,
    newest_message_id: "m1",
    ...overrides,
  };
}

describe("UpdateLineRow's gist", () => {
  const receipt = () =>
    lineFixture({
      fact: "Your receipt from Perplexity",
      fact_source: "subject",
      delta: undefined,
      numbers: [],
    });

  test("the subject shows until the gist arrives, then the gist takes its place", () => {
    const { row } = renderLine(receipt(), { focused: false });
    const fact = within(row).getByTestId("update-fact");
    expect(fact).toHaveTextContent("Your receipt from Perplexity");
    expect(fact).not.toHaveAttribute("data-gist");

    act(() => putGist(receiptGist()));
    expect(fact).toHaveTextContent("Receipt: $24.00 for Perplexity Pro, renews 14 Nov");
    expect(fact).not.toHaveTextContent("Your receipt from Perplexity");
    expect(fact).toHaveAttribute("data-gist", "true");
    // The subject is still one hover away.
    expect(fact).toHaveAttribute("title", "Your receipt from Perplexity");
    // No spinner.
    expect(within(row).queryByRole("progressbar")).toBeNull();
  });

  test("an amount the gist already says is not repeated in the trailing slot", () => {
    const line = lineFixture({
      fact: "Your receipt from Perplexity",
      delta: undefined,
      numbers: [{ raw: "$24.00", value: 24, unit: "USD" }],
    });
    const { row } = renderLine(line, { focused: false });
    expect(within(row).getByTestId("update-amount")).toHaveTextContent("$24.00");
    act(() => putGist(receiptGist()));
    expect(within(row).queryByTestId("update-amount")).toBeNull();
  });

  test("the row never shows the gist's ask; the peek may", () => {
    act(() => putGist(receiptGist({ ask: { summary: "update the card", quote: null } })));
    const { row } = renderLine(receipt(), { focused: false, peeked: true });
    expect(within(row).getByTestId("update-fact")).not.toHaveTextContent("update the card");
    expect(within(row).getByTestId("update-gist-ask")).toHaveTextContent("Asks: update the card");
  });

  test("a gist for another email of the thread, or with no email named, is not shown", () => {
    act(() => putGist(receiptGist({ newest_message_id: "m0" })));
    const { row } = renderLine(receipt(), { focused: false });
    expect(within(row).getByTestId("update-fact")).toHaveTextContent(
      "Your receipt from Perplexity",
    );
    act(() => putGist(receiptGist({ newest_message_id: null })));
    expect(within(row).getByTestId("update-fact")).toHaveTextContent(
      "Your receipt from Perplexity",
    );
  });

  test("a fact the rules took from the body keeps the rule's words", () => {
    act(() => putGist(receiptGist()));
    const { row } = renderLine(
      lineFixture({ fact: "Your payment of $19.00 was received", fact_source: "body" }),
      { focused: false },
    );
    expect(within(row).getByTestId("update-fact")).toHaveTextContent(
      "Your payment of $19.00 was received",
    );
  });

  test("a fact taken from an older email than the line's latest keeps the subject", () => {
    act(() => putGist(receiptGist()));
    const { row } = renderLine(receiptFromAnOlderEmail(), { focused: false });
    expect(within(row).getByTestId("update-fact")).toHaveTextContent(
      "Your receipt from Perplexity",
    );
  });

  test("the peek names the subject, the model and the email, and previews the email's first lines", async () => {
    apiFetch.mockResolvedValue({
      thread: {},
      messages: [],
      bodies: [
        {
          message_id: "m1",
          reader_text: [
            "Hi Bhekani,",
            "",
            "Thanks for your payment to Perplexity.",
            "Pro plan, monthly   $24.00",
            "https://perplexity.example/invoice/123",
            "<img src=x onerror=alert(1)> Renews on 14 November 2026",
            "Questions? Reply to this email.",
            "Extra line that is beyond the fourth",
          ].join("\n"),
        },
      ],
    });
    act(() => putGist(receiptGist()));
    const { row } = renderLine(receipt(), { focused: false, peeked: true });
    const peek = within(row).getByTestId("update-peek");
    expect(within(peek).getByTestId("update-subject")).toHaveTextContent(
      "Subject: Your receipt from Perplexity",
    );
    expect(within(peek).getByTestId("update-gist-source")).toHaveTextContent(
      "gemma4 · local · from this email",
    );
    const preview = await within(peek).findByTestId("update-preview");
    expect(preview.textContent).toBe(
      [
        "Thanks for your payment to Perplexity.",
        "Pro plan, monthly $24.00",
        "<img src=x onerror=alert(1)> Renews on 14 November 2026",
        "Questions? Reply to this email.",
      ].join("\n"),
    );
    // Text, never markup.
    expect(preview.querySelector("img")).toBeNull();
    expect(apiFetch).toHaveBeenCalledWith("/api/v1/mail/threads/t1");
  });

  test("a cloud model is named as cloud, and the email is the only thing it read", () => {
    act(() =>
      putGist(
        receiptGist({
          provenance: {
            model: "claude-haiku-5-5 via Anthropic",
            locality: "cloud",
            sources: ["this_thread"],
          },
        }),
      ),
    );
    const { row } = renderLine(receipt(), { focused: false, peeked: true });
    expect(within(row).getByTestId("update-gist-source")).toHaveTextContent(
      "claude-haiku-5-5 via Anthropic · cloud · from this email only",
    );
  });

  test("a closed peek fetches nothing", () => {
    renderLine(receipt(), { focused: false });
    expect(apiFetch).not.toHaveBeenCalled();
  });

  test("a peek whose fetch fails still shows its other lines", async () => {
    apiFetch.mockRejectedValue(new Error("offline"));
    const { row } = renderLine(receipt(), { peeked: true });
    await waitFor(() => expect(apiFetch).toHaveBeenCalled());
    expect(within(row).getByTestId("update-why")).toBeInTheDocument();
    expect(within(row).queryByTestId("update-preview")).toBeNull();
  });
});

function receiptFromAnOlderEmail(): UpdateLine {
  return lineFixture({
    fact: "Your receipt from Perplexity",
    fact_source: "subject",
    fact_message_id: "m0",
    latest_message_id: "m1",
    delta: undefined,
    numbers: [],
  });
}

describe("UpdateLineRow with a one-time code", () => {
  test("the code shows on the line and a click copies it without opening the peek", async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>().mockResolvedValue();
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const { row, handlers } = renderLine(
      lineFixture({ source_name: "Acme", fact: "MFA Login Authentication", code: "882308" }),
    );
    const code = within(row).getByTestId("update-code");
    expect(code).toHaveTextContent("882308");
    fireEvent.click(code);
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("882308"));
    expect(handlers.onTogglePeek).not.toHaveBeenCalled();
    // The code itself and the `y` verb beside the others.
    expect(within(row).getAllByRole("button", { name: /copy code/i })).toHaveLength(2);
  });

  test("a line without a code offers no copy", () => {
    const { row } = renderLine(lineFixture());
    expect(within(row).queryByTestId("update-code")).toBeNull();
  });
});
