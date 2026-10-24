/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render as renderPlain,
  renderHook,
  screen,
  within,
  type RenderOptions,
} from "@testing-library/react";
import type { ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type {
  AssistantConversation,
  AssistantDecision,
  AssistantItem,
  AssistantPending,
} from "./api";
import { previewMessages } from "./describe";
import { renderMarkdown } from "./markdown";
import { PendingCards } from "./PendingCards";
import { orderedOptions } from "./Question";
import { Transcript, mutationIdOf } from "./Transcript";
import { useLandChanges } from "./useAssistant";
import { foldSummary, irreversibleWarning, liveLine, slowNote, stepTitle } from "./wire";
import { LiveLine } from "./LiveLine";
import { useLanded, useLanding } from "@/state/landingStore";

const navigate = vi.fn<(options: { href: string }) => Promise<void>>();
vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  useNavigate: () => navigate,
}));
const invalidate = vi.fn<() => Promise<void>>();
vi.mock("@/features/mail-actions/mailQueryInvalidation", () => ({
  invalidateMailQueries: () => invalidate(),
}));

// Email chips look up where their thread lives; each test gets its own cache.
const apiFetch = vi.hoisted(() => vi.fn<(path: string, init?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch,
}));
/** What the membership lookups asked for, one entry per request. */
const lookups = () =>
  apiFetch.mock.calls.map(
    ([, init]) => (init as { body: { thread_ids: string[] } }).body.thread_ids,
  );
const answerLookups = (threads: unknown[]) =>
  apiFetch.mockResolvedValue({ kind: "ModeMembership", threads });

function holdLookup() {
  const held = {} as { promise: Promise<unknown>; resolve: (answer: unknown) => void };
  held.promise = new Promise((resolve) => (held.resolve = resolve));
  return held;
}

function render(ui: ReactElement, options?: RenderOptions) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return renderPlain(ui, {
    wrapper: ({ children }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
    ...options,
  });
}

beforeEach(() => {
  apiFetch.mockReset();
  answerLookups([]);
});

const approval: AssistantPending = {
  kind: "approval",
  call_id: "c1",
  tool: "mxr_mutate",
  title: "Archive 2 emails",
  arguments: { action: "archive", message_ids: ["m1", "m2"], confirm: true },
  reason: "Archive the two shipping notices.",
  preview: {
    dry_run: true,
    preview: {
      kind: "Envelopes",
      envelopes: [
        {
          id: "m1",
          subject: "Your parcel shipped",
          from: { name: "Shop", email: "s@x" },
          date: "2026-10-14T09:00:00Z",
        },
        {
          id: "m2",
          subject: "Out for delivery",
          from: { email: "s@x" },
          date: "2026-10-15T07:00:00Z",
        },
      ],
    },
  },
};

const question: AssistantPending = {
  kind: "question",
  call_id: "q1",
  reason: "Two accounts match.",
  questions: [
    {
      question: "Which account?",
      header: "Account",
      options: [{ label: "Work" }, { label: "Home (Recommended)", description: "Personal Gmail" }],
      multi_select: false,
      allow_other: true,
    },
  ],
};

function cards(pending: AssistantPending[], busy = false) {
  const onDecide = vi.fn<(decision: AssistantDecision) => void>();
  render(<PendingCards pending={pending} busy={busy} onDecide={onDecide} />);
  return onDecide;
}

describe("proof", () => {
  test("names the change in the daemon's words and lists what it touches as rows", () => {
    cards([approval]);
    expect(screen.getByRole("heading", { name: "Archive 2 emails" })).toBeVisible();
    expect(screen.getByText("Archive the two shipping notices.")).toBeVisible();
    const rows = within(screen.getByRole("list", { name: "What this changes" }));
    expect(rows.getByText("Your parcel shipped")).toBeVisible();
    expect(rows.getByText("Shop")).toBeVisible();
    expect(rows.getByText("Out for delivery")).toBeVisible();
  });

  test("a change with no title says so plainly, never a tool name", () => {
    cards([{ ...approval, title: undefined } as AssistantPending]);
    expect(screen.getByRole("heading", { name: "Change waiting for you" })).toBeVisible();
  });

  test("shows the daemon's warning, and works one out only when it sends none", () => {
    const { unmount } = render(
      <PendingCards
        pending={[{ ...approval, tool: "mxr_mutate", warning: "From the daemon." }]}
        busy={false}
        onDecide={vi.fn<(decision: AssistantDecision) => void>()}
      />,
    );
    expect(screen.getByText("From the daemon.")).toBeVisible();
    unmount();
    // An older daemon: no field, so the client's own rule for a trash.
    cards([{ ...approval, arguments: { action: "trash", message_ids: ["m1"] } }]);
    expect(screen.getByText("Can't be undone.")).toBeVisible();
  });

  test("a reversible change shows no warning", () => {
    cards([approval]);
    expect(screen.queryByText(/undone/)).toBeNull();
  });

  test("has one primary button, Apply, and it approves", () => {
    const onDecide = cards([approval]);
    const apply = screen.getByRole("button", { name: /^Apply/ });
    expect(apply).toHaveAttribute("data-slot", "button");
    // Everything else on the card is quiet: no second filled button.
    const filled = screen
      .getAllByRole("button")
      .filter((button) => button.className.includes("bg-primary"));
    expect(filled).toEqual([apply]);
    fireEvent.click(apply);
    expect(onDecide).toHaveBeenCalledWith({ call_id: "c1", decision: "approve" });
  });

  test("y applies, n opens the note, x shows the exact call", () => {
    const onDecide = cards([approval]);
    const card = screen.getByRole("region", { name: "Archive 2 emails" });
    fireEvent.keyDown(card, { key: "x" });
    expect(screen.getByText(/mxr_mutate/)).toBeVisible();
    fireEvent.keyDown(card, { key: "n" });
    expect(screen.getByLabelText("Why, or what to do instead")).toBeVisible();
    expect(onDecide).not.toHaveBeenCalled();
  });

  test("y applies from the card", () => {
    const onDecide = cards([approval]);
    fireEvent.keyDown(screen.getByRole("region", { name: "Archive 2 emails" }), { key: "y" });
    expect(onDecide).toHaveBeenCalledWith({ call_id: "c1", decision: "approve" });
  });

  test("keys typed into the note field never decide, and a doubled y decides once", () => {
    const onDecide = cards([approval]);
    const card = screen.getByRole("region", { name: "Archive 2 emails" });
    fireEvent.click(screen.getByRole("button", { name: /send back with a note/ }));
    const field = screen.getByLabelText("Why, or what to do instead");
    for (const key of ["y", "n", "x"]) fireEvent.keyDown(field, { key });
    expect(onDecide).not.toHaveBeenCalled();
    fireEvent.keyDown(field, { key: "Escape" });
    fireEvent.keyDown(card, { key: "y" });
    fireEvent.keyDown(card, { key: "y" });
    expect(onDecide).toHaveBeenCalledTimes(1);
  });

  test("a held key, a chord and a busy card do nothing", () => {
    const onDecide = cards([approval], true);
    const card = screen.getByRole("region", { name: "Archive 2 emails" });
    fireEvent.keyDown(card, { key: "y" });
    expect(onDecide).not.toHaveBeenCalled();
  });

  test("a chord with y is not Apply", () => {
    const onDecide = cards([approval]);
    const card = screen.getByRole("region", { name: "Archive 2 emails" });
    fireEvent.keyDown(card, { key: "y", metaKey: true });
    fireEvent.keyDown(card, { key: "y", repeat: true });
    expect(onDecide).not.toHaveBeenCalled();
  });

  test("a note goes back to the model as the reason", () => {
    const onDecide = cards([approval]);
    fireEvent.click(screen.getByRole("button", { name: /send back with a note/ }));
    fireEvent.change(screen.getByLabelText("Why, or what to do instead"), {
      target: { value: "keep the second one" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send back" }));
    expect(onDecide).toHaveBeenCalledWith({
      call_id: "c1",
      decision: "reject",
      reason: "keep the second one",
    });
  });

  test("an irreversible change says so above Apply; a reversible one does not", () => {
    cards([
      {
        ...approval,
        call_id: "c2",
        tool: "mxr_send_draft",
        title: "Send an email to “ann@x.com”",
        arguments: { draft_id: "d1" },
        preview: { draft: { to: [{ email: "ann@x.com" }], subject: "Hi", body: "Hello Ann" } },
      },
    ]);
    expect(screen.getByText("Sends an email. Can't be undone.")).toBeVisible();
    expect(screen.getByText("Hello Ann")).toBeVisible();
    expect(irreversibleWarning("mxr_mutate", { action: "archive" })).toBeNull();
    expect(irreversibleWarning("mxr_mutate", { action: "trash" })).toBe("Can't be undone.");
  });

  test("more than five rows end in 'and N more', which opens the rest", () => {
    const envelopes = Array.from({ length: 8 }, (_, n) => ({
      id: `m${n}`,
      subject: `Subject ${n}`,
      from: { name: `Sender ${n}`, email: "s@x" },
    }));
    cards([{ ...approval, preview: { preview: { envelopes } } } as AssistantPending]);
    expect(screen.queryByText("Subject 7")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "and 3 more" }));
    expect(screen.getByText("Subject 7")).toBeVisible();
  });
});

describe("question", () => {
  test("recommended goes first and is tagged; the model's marker is not shown", () => {
    cards([question]);
    const options = screen.getAllByRole("radio");
    expect(options[0]).toHaveTextContent("Home");
    expect(options[0]).toHaveTextContent("recommended");
    expect(options[0]).not.toHaveTextContent("(Recommended)");
    expect(
      orderedOptions(question.kind === "question" ? question.questions[0]! : (null as never)).map(
        (o) => o.label,
      ),
    ).toEqual(["Home", "Work"]);
  });

  test("answers with the chosen option, and only once something is chosen", () => {
    const onDecide = cards([question]);
    const answer = screen.getByRole("button", { name: /^Answer/ });
    expect(answer).toBeDisabled();
    const group = screen.getByRole("radiogroup", { name: "Which account?" });
    fireEvent.click(within(group).getByRole("radio", { name: /Work/ }));
    expect(within(group).getByRole("radio", { name: /Work/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    fireEvent.click(answer);
    expect(onDecide).toHaveBeenCalledWith({
      call_id: "q1",
      decision: "answer",
      answers: [{ selected: ["Work"], other: undefined }],
    });
  });

  test("number keys choose, but not while typing in 'Something else'", () => {
    const onDecide = cards([question]);
    const card = screen.getByRole("region", { name: "A question for you" });
    fireEvent.keyDown(screen.getByLabelText("Something else"), { key: "1" });
    expect(
      screen.getAllByRole("radio").every((radio) => radio.getAttribute("aria-checked") === "false"),
    ).toBe(true);
    fireEvent.keyDown(card, { key: "1" });
    expect(screen.getAllByRole("radio")[0]).toHaveAttribute("aria-checked", "true");
    expect(onDecide).not.toHaveBeenCalled();
  });

  test("typing an answer replaces a single choice, and Skip declines", () => {
    const onDecide = cards([question]);
    fireEvent.click(screen.getByRole("radio", { name: /Work/ }));
    fireEvent.change(screen.getByLabelText("Something else"), { target: { value: "Both" } });
    fireEvent.click(screen.getByRole("button", { name: /^Answer/ }));
    expect(onDecide).toHaveBeenLastCalledWith({
      call_id: "q1",
      decision: "answer",
      answers: [{ selected: [], other: "Both" }],
    });
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(onDecide).toHaveBeenLastCalledWith({ call_id: "q1", decision: "reject" });
  });
});

const search: AssistantItem = {
  kind: "tool_call",
  seq: 2,
  call_id: "a",
  tool: "mxr_search",
  reason: "Find shipping notices.",
  title: { running: "Searching your mail for “shipped”", done: "Searched your mail: 14 found" },
  arguments: { query: "shipped" },
  state: "done",
  result: '{"total":14}',
  readout: {
    total: 14,
    rows: [
      {
        title: "Your parcel shipped",
        detail: "Shop",
        link: { kind: "thread", thread_id: "t-1234567" },
      },
    ],
  },
  created_at: 2,
};
const turn: AssistantItem[] = [
  { kind: "user", seq: 1, text: "tidy shipping mail", created_at: 1 },
  search,
  { kind: "assistant", seq: 3, text: "Found **14**.", model: "gemma4", created_at: 3 },
];

describe("transcript", () => {
  test("your words are quoted, not bubbled", () => {
    render(<Transcript items={turn} running={false} conversationId="c1" />);
    const quote = screen.getByText("tidy shipping mail");
    expect(quote.className).toContain("border-l-2");
    expect(quote.className).not.toContain("bg-");
  });

  test("a finished turn folds to one line that opens to wire lines and a readout", () => {
    render(<Transcript items={turn} running={false} conversationId="c1" />);
    const fold = screen.getByRole("button", { name: /1 step · 14 found/ });
    expect(screen.queryByText("Searched your mail: 14 found")).toBeNull();
    fireEvent.click(fold);
    fireEvent.click(screen.getByRole("button", { name: /Searched your mail: 14 found/ }));
    expect(screen.getByText("Found 14")).toBeVisible();
    expect(screen.getByText("and 13 more")).toBeVisible();
    expect(screen.getByRole("link", { name: /Your parcel shipped/ })).toBeVisible();
  });

  test("no default view shows a tool name or JSON, until Details is opened", () => {
    const { container } = render(<Transcript items={turn} running={false} conversationId="c1" />);
    fireEvent.click(screen.getByRole("button", { name: /1 step/ }));
    fireEvent.click(screen.getByRole("button", { name: /Searched your mail/ }));
    expect(container.textContent).not.toMatch(/mxr_[a-z_]+|\{\s*"/);
    fireEvent.click(screen.getByRole("button", { name: /Details/ }));
    expect(container.textContent).toMatch(/mxr_search/);
  });

  test("a step reads in the tense of its state", () => {
    expect(stepTitle({ ...search, state: "running" } as never)).toBe(
      "Searching your mail for “shipped”",
    );
    expect(stepTitle(search as never)).toBe("Searched your mail: 14 found");
  });

  test("an old daemon sends no titles: the model's reason, else the tool as words", () => {
    const old = { ...search, title: undefined } as never;
    expect(stepTitle(old)).toBe("Find shipping notices.");
    expect(stepTitle({ ...(old as object), reason: "" } as never)).toBe("Search");
  });

  test("an answered question collapses into your quoted reply", () => {
    const asked: AssistantItem = {
      kind: "tool_call",
      seq: 2,
      call_id: "q",
      tool: "ask_user",
      reason: "",
      title: { running: "Asking you a question", done: "You answered a question" },
      arguments: {},
      state: "answered",
      result: '{"answers":[{"selected":["Newsletters and receipts"]}]}',
      created_at: 2,
    };
    render(<Transcript items={[turn[0]!, asked]} running={false} conversationId="c1" />);
    expect(screen.getByText("Newsletters and receipts").className).toContain("border-l-2");
    expect(screen.queryByRole("button", { name: /1 step/ })).toBeNull();
  });

  test("the fold counts changes waiting and failures", () => {
    const base = { ...search, readout: undefined } as never as Extract<
      AssistantItem,
      { kind: "tool_call" }
    >;
    const sum = foldSummary([
      search as never,
      { ...base, call_id: "w", state: "waiting" },
      { ...base, call_id: "f", state: "failed" },
    ]);
    expect(sum).toEqual({ steps: 3, found: 14, changed: 0, waiting: 1, failed: 1 });
  });

  test("a failed step shows under the reply in the daemon's words, whatever the reply says", () => {
    const failed: AssistantItem = {
      kind: "tool_call",
      seq: 4,
      call_id: "b",
      tool: "mxr_mutate",
      reason: "Star it.",
      title: { running: "Starring 1 email", done: "Couldn't star 1 email: it was already moved" },
      arguments: { action: "star", message_ids: ["m1"] },
      state: "failed",
      result: '{"blocked":true}',
      created_at: 4,
    };
    const reply = { ...turn[2]!, failed_call_ids: ["b"] } as AssistantItem;
    render(
      <Transcript items={[turn[0]!, search, failed, reply]} running={false} conversationId="c1" />,
    );
    expect(screen.getByRole("button", { name: /2 steps.*1 failed/ })).toBeVisible();
    const note = screen.getByRole("note");
    expect(note).toHaveTextContent("These didn't happen, whatever the reply says:");
    expect(note).toHaveTextContent("Couldn't star 1 email: it was already moved");
  });

  test("a reply whose steps all worked has no failure note", () => {
    render(<Transcript items={turn} running={false} conversationId="c1" />);
    expect(screen.queryByRole("note")).toBeNull();
  });

  test("the reply carries the model mark, and where it ran when that model is the current one", () => {
    render(
      <Transcript
        conversationId="c1"
        items={turn}
        running={false}
        provenance={{ model: "gemma4", leavesMachine: false }}
      />,
    );
    expect(screen.getByTestId("model-mark")).toHaveTextContent("gemma4 · local");
  });

  test("a reply from another model says only which", () => {
    render(
      <Transcript
        conversationId="c1"
        items={turn}
        running={false}
        provenance={{ model: "gpt-x", leavesMachine: true }}
      />,
    );
    expect(screen.getByTestId("model-mark")).toHaveTextContent("gemma4");
    expect(screen.getByTestId("model-mark")).not.toHaveTextContent("cloud");
  });

  test("a reply keeps the place its own model ran after the model in use changes", () => {
    const reply = {
      ...turn[2]!,
      model: "claude-haiku-5-5 via Anthropic",
      leaves_machine: true,
    } as AssistantItem;
    render(
      <Transcript
        conversationId="c1"
        items={[turn[0]!, turn[1]!, reply]}
        running={false}
        provenance={{ model: "gemma4", leavesMachine: false }}
      />,
    );
    expect(screen.getByTestId("model-mark")).toHaveTextContent(
      "claude-haiku-5-5 via Anthropic · cloud",
    );
  });

  test("rows a reply refers to draw under it, each opening beside the assistant", () => {
    navigate.mockClear();
    const reply = {
      ...turn[2]!,
      rows: [
        { title: "Q1 report", detail: "Diana", link: { kind: "thread", thread_id: "t-9" } },
        {
          title: "Nigel Nk",
          detail: "Visa pack",
          link: { kind: "person", person_id: "person:n@x.com" },
        },
      ],
    } as AssistantItem;
    render(<Transcript items={[turn[0]!, reply]} running={false} conversationId="c1" />);
    const rows = within(screen.getByRole("list", { name: "The emails and people in this reply" }));
    fireEvent.click(rows.getByRole("link", { name: /Nigel Nk/ }));
    expect(navigate).toHaveBeenCalledWith({ href: "/messages?person=person%3An%40x.com" });
  });

  describe("email rows", () => {
    const replyWith = (rows: unknown[]) => ({ ...turn[2]!, rows }) as AssistantItem;
    const list = () =>
      within(screen.getByRole("list", { name: "The emails and people in this reply" }));
    const rowNow = new Date(2026, 9, 20, 12);
    afterEach(() => vi.useRealTimers());
    const rich = {
      title: "Invoice 4471 overdue",
      detail: "Dana Ruiz",
      sender: "Dana Ruiz",
      snippet: "Please confirm payment by Friday",
      at: Math.floor(rowNow.getTime() / 1000) - 10 * 60,
      attachment: true,
      unread: true,
      link: { kind: "thread", thread_id: "t-rich" },
    };

    test("show the sender, the start of the text, a clip, the mode glyph and the time", async () => {
      vi.useFakeTimers({ toFake: ["Date"], now: rowNow });
      answerLookups([
        {
          thread_id: "t-rich",
          in_inbox: false,
          modes: [{ mode: "reading", name: "Reading" }],
          account_id: "a",
          subject: "x",
        },
      ]);
      render(
        <Transcript items={[turn[0]!, replyWith([rich])]} running={false} conversationId="c1" />,
      );
      const row = list().getByRole("link", { name: /Invoice 4471 overdue/ });
      expect(row).toHaveTextContent("Dana Ruiz · Please confirm payment by Friday");
      const li = row.closest("li")!;
      expect(li.querySelector(".lucide-paperclip")).not.toBeNull();
      expect(li).toHaveTextContent("10m");
      expect(li.dataset.unread).toBe("true");
      await vi.waitFor(() => expect(li.querySelector(".lucide-newspaper")).not.toBeNull());
      expect(lookups()).toEqual([["t-rich"]]);
    });

    test("a row with only a detail draws as before and asks for nothing", () => {
      render(
        <Transcript
          items={[
            turn[0]!,
            replyWith([
              { title: "Q1 report", detail: "Diana", link: { kind: "thread", thread_id: "t-9" } },
            ]),
          ]}
          running={false}
          conversationId="c1"
        />,
      );
      const li = list()
        .getByRole("link", { name: /Q1 report/ })
        .closest("li")!;
      expect(li).toHaveTextContent("Q1 report");
      expect(li).toHaveTextContent("Diana");
      expect(li.querySelector("svg.lucide-paperclip, svg.lucide-mail")).toBeNull();
      expect(li.dataset.unread).toBeUndefined();
      expect(lookups()).toEqual([]);
    });

    test("hostile mail text stays text", () => {
      const hostile = '<img src=x onerror="alert(1)"><b>bold</b>';
      render(
        <Transcript
          items={[
            turn[0]!,
            replyWith([{ ...rich, title: hostile, sender: hostile, snippet: hostile }]),
          ]}
          running={false}
          conversationId="c1"
        />,
      );
      const rows = screen.getByRole("list", { name: "The emails and people in this reply" });
      expect(rows.querySelector("img, b")).toBeNull();
      expect(rows).toHaveTextContent(hostile);
    });
  });

  test("shows steps as they happen while the turn runs", () => {
    render(<Transcript items={turn.slice(0, 2)} running conversationId="c1" />);
    expect(screen.getByRole("button", { name: /Searched your mail: 14 found/ })).toBeVisible();
  });

  test("a summary marks where earlier messages were folded, and auto-approved steps say so", () => {
    render(
      <Transcript
        conversationId="c1"
        items={[
          { kind: "user", seq: 1, text: "old", created_at: 1 },
          { kind: "summary", seq: 1, text: "They asked about invoices.", created_at: 2 },
          {
            kind: "tool_call",
            seq: 2,
            call_id: "a",
            tool: "mxr_mutate",
            reason: "Archive the receipts.",
            title: {
              running: "Archiving 2 emails",
              done: "Archived 2 emails · ran without asking",
            },
            arguments: {},
            state: "auto_approved",
            created_at: 2,
          },
        ]}
        running
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Earlier messages summarised/ }));
    expect(screen.getByText("They asked about invoices.")).toBeVisible();
    expect(screen.getByRole("img", { name: "Ran without asking" })).toBeInTheDocument();
  });

  test("the reply being written shows while the turn runs, and is gone once stored", () => {
    const { rerender } = render(
      <Transcript
        conversationId="c1"
        items={[{ kind: "user", seq: 1, text: "hi", created_at: 1 }]}
        running
        writing="Looking at **your** mail"
      />,
    );
    expect(screen.getByText("your").tagName).toBe("STRONG");
    rerender(
      <Transcript
        conversationId="c1"
        items={[{ kind: "user", seq: 1, text: "hi", created_at: 1 }]}
        running={false}
        writing={null}
      />,
    );
    expect(screen.queryByText("your")).toBeNull();
  });
});

describe("live line", () => {
  test("says what the desk is doing in the daemon's words", () => {
    expect(liveLine({ kind: "thinking", step: 1 })).toBe("Reading your request");
    expect(liveLine({ kind: "thinking", step: 3 })).toBe("Deciding what to do next");
    expect(
      liveLine({
        kind: "calling",
        tool: "mxr_search",
        reason: "x",
        title: "Searching your mail for invoices",
      }),
    ).toBe("Searching your mail for invoices");
    expect(liveLine({ kind: "writing", text: "Hi" })).toBe("Writing the answer");
    expect(liveLine({ kind: "summarizing" })).toBe("Making room: summarising earlier messages");
  });

  test("an old daemon sends no call title: the reason stands in", () => {
    expect(liveLine({ kind: "calling", tool: "mxr_search", reason: "Looking for invoices" })).toBe(
      "Looking for invoices",
    );
  });

  test("names the model only after a step runs long", () => {
    expect(slowNote("gemma4", 19)).toBeNull();
    expect(slowNote("gemma4", 24)).toBe("gemma4 is slow on this; 24s");
    expect(slowNote(null, 90)).toBeNull();
  });

  test("shows the phrase, the seconds and Stop; Stop stops", () => {
    vi.useFakeTimers();
    const onStop = vi.fn<() => void>();
    render(
      <LiveLine
        progress={{ kind: "thinking", step: 1 }}
        model="gemma4"
        startedAt={Date.now()}
        stopping={false}
        onStop={onStop}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Reading your request");
    act(() => {
      vi.advanceTimersByTime(24_000);
    });
    expect(screen.getByRole("status")).toHaveTextContent("gemma4 is slow on this; 24s");
    fireEvent.click(screen.getByRole("button", { name: /Stop/ }));
    expect(onStop).toHaveBeenCalled();
    vi.useRealTimers();
  });
});

describe("markdown", () => {
  test("drops scripts and images and opens links safely", () => {
    const html = renderMarkdown(
      "Hi <script>alert(1)</script> ![pixel](https://t.example/p.gif) [site](https://ok.example) [bad](javascript:alert(1))",
    );
    expect(html).not.toContain("<script");
    expect(html).not.toContain("<img");
    expect(html).toContain('href="https://ok.example"');
    expect(html).toContain('rel="noopener noreferrer nofollow"');
    expect(html).not.toContain("javascript:");
  });

  test("keeps email addresses as text rather than mailto links", () => {
    const html = renderMarkdown("Write to quinn@work.com or [me](mailto:me@x.example).");
    expect(html).not.toContain("mailto:");
    expect(html).not.toContain("<a");
    expect(html).toContain("quinn@work.com");
  });

  test("mail text in a reply is text, never markup", () => {
    render(
      <Transcript
        conversationId="c1"
        running={false}
        items={[
          {
            kind: "assistant",
            seq: 1,
            text: "Subject: <img src=x onerror=alert(1)> <b onclick=alert(2)>bold</b>",
            created_at: 0,
          },
        ]}
      />,
    );
    expect(document.querySelector("img")).toBeNull();
    expect(document.querySelector("[onclick]")).toBeNull();
  });
});

describe("links to mail", () => {
  test("a stored reply's mxr links open in the app; a reply being written shows words", () => {
    const reply =
      "Reply to [Q1 report](mxr:thread/t-1234567) and [Diana](mxr:person/person:d@x.com).";
    const stored = renderMarkdown(reply, { appLinks: true });
    expect(stored).toContain('href="/m/archive/t-1234567"');
    expect(stored).toContain('href="/messages?person=person%3Ad%40x.com"');
    expect(stored).toContain("data-app-link");
    expect(stored).not.toContain("target=");
    const writing = renderMarkdown(reply);
    expect(writing).not.toContain("<a");
    expect(writing).toContain("Q1 report");
  });

  test("an app-link marker the model wrote itself is dropped, so an outside link never looks like an email", () => {
    const html = renderMarkdown(
      'See <a href="https://evil.test" data-app-link="thread" data-app-link-id="t-1">Q1 report</a>.',
      { appLinks: true },
    );
    expect(html).toContain('href="https://evil.test"');
    expect(html).toContain('target="_blank"');
    expect(html).not.toContain("data-app-link");
  });

  test("people and emails are chips, not underlined links, and clicking navigates in the app", () => {
    navigate.mockClear();
    render(
      <Transcript
        conversationId="c1"
        running={false}
        items={[
          {
            kind: "assistant",
            seq: 1,
            text: "See [Q1 report](mxr:thread/t-1234567) from [Diana Lee](mxr:person/person:d@x.com).",
            created_at: 0,
          },
        ]}
      />,
    );
    const email = screen.getByRole("link", { name: "Q1 report" });
    const person = screen.getByRole("link", { name: /Diana Lee/ });
    expect(email.className).toContain("assistant-chip");
    expect(person.className).toContain("assistant-chip");
    expect(person.querySelector("span[aria-hidden]")?.className).toContain("size-4");
    fireEvent.click(email);
    expect(navigate).toHaveBeenCalledWith({ href: "/m/archive/t-1234567" });
  });

  const chipFor = (text: string, modes: unknown[] | null) => {
    answerLookups(
      modes === null
        ? []
        : [{ thread_id: "t-1234567", in_inbox: false, modes, account_id: "a", subject: "x" }],
    );
    render(
      <Transcript
        conversationId="c1"
        items={[
          {
            kind: "assistant",
            seq: 1,
            text: `See [Q1 report](mxr:thread/t-1234567). ${text}`,
            created_at: 0,
          },
        ]}
        running={false}
      />,
    );
    return screen.getByRole("link", { name: "Q1 report" });
  };

  test("an email chip wears the glyph of the mode that holds it", async () => {
    const chip = chipFor("", [{ mode: "reading", name: "Reading" }]);
    expect(chip.querySelector(".lucide-mail")).not.toBeNull();
    await vi.waitFor(() => expect(chip.querySelector(".lucide-newspaper")).not.toBeNull());
    expect(chip.querySelector(".lucide-mail")).toBeNull();
  });

  test("a chip for an email no mode knows keeps the mail icon", async () => {
    const chip = chipFor("", null);
    await vi.waitFor(() => expect(apiFetch).toHaveBeenCalled());
    expect(chip.querySelector(".lucide-mail")).not.toBeNull();
  });

  test("every chip in a reply is looked up in one request", async () => {
    render(
      <Transcript
        conversationId="c1"
        items={[
          {
            kind: "assistant",
            seq: 1,
            text: "[A](mxr:thread/t-a) and [B](mxr:thread/t-b) and [A again](mxr:thread/t-a)",
            created_at: 0,
          },
        ]}
        running={false}
      />,
    );
    await vi.waitFor(() => expect(lookups()).toEqual([["t-a", "t-b"]]));
  });

  test("a re-render while the lookup is out keeps the chip and settles on the glyph", async () => {
    const held = holdLookup();
    const answer = (threads: unknown[]) => held.resolve({ kind: "ModeMembership", threads });
    apiFetch.mockReturnValue(held.promise);
    const items = [
      {
        kind: "assistant" as const,
        seq: 1,
        text: "See [Q1 report](mxr:thread/t-1234567).",
        created_at: 0,
      },
    ];
    const { rerender } = render(<Transcript items={items} running={false} conversationId="c1" />);
    rerender(<Transcript items={[...items]} running={false} conversationId="c1" />);
    const chip = screen.getByRole("link", { name: "Q1 report" });
    expect(chip.querySelector(".lucide-mail")).not.toBeNull();
    await act(async () => {
      answer([{ thread_id: "t-1234567", in_inbox: false, modes: [{ mode: "todo" }] }]);
    });
    await vi.waitFor(() =>
      expect(
        screen.getByRole("link", { name: "Q1 report" }).querySelector(".lucide-list-checks"),
      ).not.toBeNull(),
    );
    expect(lookups()).toHaveLength(1);
  });
});

describe("change titles and previews", () => {
  test("a preview's messages", () => {
    expect(previewMessages(approval.kind === "approval" ? approval.preview : null)).toHaveLength(2);
    expect(previewMessages({ nothing: true })).toEqual([]);
  });
});

describe("undo", () => {
  test("an approved mail change can be undone by its mutation id; reads can't", () => {
    const base = {
      kind: "tool_call" as const,
      seq: 1,
      call_id: "a",
      tool: "mxr_mutate",
      reason: "",
      arguments: {},
      created_at: 1,
    };
    const result = '{"kind":"MutationResult","result":{"mutation_id":"01abc","succeeded":2}}';
    expect(mutationIdOf({ ...base, state: "approved", result })).toBe("01abc");
    expect(mutationIdOf({ ...base, state: "auto_approved", result })).toBe("01abc");
    expect(mutationIdOf({ ...base, state: "done", result })).toBeNull();
    expect(mutationIdOf({ ...base, state: "approved", result: "{}" })).toBeNull();
  });
});

describe("landing", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invalidate.mockClear();
    useLanding.setState({ landed: new Set() });
  });
  afterEach(() => vi.useRealTimers());

  const conversation = (
    calls: Array<{ id: string; state: "waiting" | "approved" }>,
  ): AssistantConversation =>
    ({
      id: "conv",
      title: "t",
      status: "idle",
      pending: [],
      created_at: 0,
      updated_at: 0,
      items: calls.map((call, n) => ({
        kind: "tool_call",
        seq: n,
        call_id: call.id,
        tool: "mxr_mutate",
        reason: "",
        arguments: { action: "archive", message_ids: ["m1", "m2"] },
        state: call.state,
        created_at: 0,
      })),
    }) as AssistantConversation;

  test("a row asks for its own ids and the mark clears after 600ms", () => {
    const { result } = renderHook(() => useLanded("t1", "m1"));
    expect(result.current).toBe(false);
    act(() => useLanding.getState().land(["m1"]));
    expect(result.current).toBe(true);
    const other = renderHook(() => useLanded("t9", "m9"));
    expect(other.result.current).toBe(false);
    act(() => {
      vi.advanceTimersByTime(600);
    });
    expect(result.current).toBe(false);
  });

  test("a row that isn't landing doesn't re-render when another does", () => {
    let renders = 0;
    renderHook(() => {
      renders += 1;
      return useLanded("t9");
    });
    const before = renders;
    act(() => useLanding.getState().land(["m1"]));
    expect(renders).toBe(before);
  });

  test("a change applied while the conversation is open lands; history doesn't replay", () => {
    const { rerender } = renderHook(({ c }) => useLandChanges(c), {
      initialProps: {
        c: conversation([
          { id: "old", state: "approved" },
          { id: "new", state: "waiting" },
        ]),
      },
    });
    expect(useLanding.getState().landed.size).toBe(0);
    rerender({
      c: conversation([
        { id: "old", state: "approved" },
        { id: "new", state: "approved" },
      ]),
    });
    expect([...useLanding.getState().landed].toSorted()).toEqual(["m1", "m2"]);
    act(() => {
      vi.advanceTimersByTime(600);
    });
    expect(invalidate).toHaveBeenCalledTimes(1);
  });
});

const rowsOf = (count: number) => Array.from({ length: count }, (_, n) => ({ title: `Mail ${n}` }));
/** Answers every request but the step's rows, which each test answers itself. */
const answerOthers = (path: string) =>
  path.includes("/steps/") ? undefined : { kind: "ModeMembership", threads: [] };

describe("step rows", () => {
  // A step whose result lists 12 emails; the readout shows the first five.
  const wide: AssistantItem = { ...search, readout: { total: 12, rows: rowsOf(5) } };
  const stepCalls = () => apiFetch.mock.calls.filter(([path]) => String(path).includes("/steps/"));
  const open = () => {
    render(<Transcript items={[turn[0]!, wide]} running={false} conversationId="c1" />);
    fireEvent.click(screen.getByRole("button", { name: /1 step/ }));
    fireEvent.click(screen.getByRole("button", { name: /Searched your mail/ }));
  };
  const STEP_PATH = "/api/v1/mail/assistant/conversations/c1/steps/a/rows?seq=2";

  test("and N more opens every row the step found, and Show fewer closes it", async () => {
    apiFetch.mockImplementation(
      async (path: string) => answerOthers(path) ?? { rows: rowsOf(12), listed: 12, total: 12 },
    );
    open();
    expect(screen.queryByText("Mail 7")).toBeNull();
    const more = screen.getByRole("button", { name: "and 7 more" });
    expect(more).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(more);
    expect(await screen.findByText("Mail 11")).toBeVisible();
    expect(stepCalls()).toEqual([[STEP_PATH]]);
    const fewer = screen.getByRole("button", { name: "Show fewer" });
    expect(fewer).toHaveAttribute("aria-expanded", "true");

    fireEvent.click(fewer);
    expect(screen.queryByText("Mail 7")).toBeNull();
    expect(screen.getByRole("button", { name: "and 7 more" })).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "and 7 more" }));
    expect(await screen.findByText("Mail 11")).toBeVisible();
    expect(stepCalls()).toHaveLength(1);
  });

  test("while the rest loads, the same button says so and keeps focus", async () => {
    const held = holdLookup();
    apiFetch.mockImplementation(async (path: string) => answerOthers(path) ?? (await held.promise));
    open();
    const more = screen.getByRole("button", { name: "and 7 more" });
    more.focus();
    fireEvent.click(more);

    const loading = screen.getByRole("button", { name: "Loading the rest…" });
    expect(loading).toBe(more);
    expect(loading).toHaveAttribute("aria-busy", "true");
    expect(document.activeElement).toBe(more);
    // A second click while loading starts no second request.
    fireEvent.click(more);
    expect(stepCalls()).toHaveLength(1);

    await act(async () => held.resolve({ rows: rowsOf(12), listed: 12, total: 12 }));
    expect(await screen.findByText("Mail 11")).toBeVisible();
    const fewer = screen.getByRole("button", { name: "Show fewer" });
    expect(fewer).toBe(more);
    expect(document.activeElement).toBe(more);
  });

  test("when the rest fails, the step keeps its five and says so", async () => {
    apiFetch.mockImplementation(async (path: string) => {
      if (path.includes("/steps/")) throw new Error("the daemon is busy");
      return answerOthers(path);
    });
    open();
    fireEvent.click(screen.getByRole("button", { name: "and 7 more" }));
    expect(await screen.findByText("Couldn't load the rest")).toBeVisible();
    expect(screen.getByText("Mail 4")).toBeVisible();
    expect(screen.queryByText("Mail 7")).toBeNull();
    expect(screen.getByRole("button", { name: "and 7 more" })).toBeVisible();
  });

  test("a total above what the rows list ends the list with a plain line", async () => {
    apiFetch.mockImplementation(
      async (path: string) => answerOthers(path) ?? { rows: rowsOf(10), listed: 10, total: 12 },
    );
    open();
    fireEvent.click(screen.getByRole("button", { name: "and 7 more" }));
    expect(await screen.findByText("Mail 9")).toBeVisible();
    expect(screen.getByText("and 2 more matched that this step didn't load")).toBeVisible();
  });

  test("a conversation switch folds the steps and drops a late response for the old one", async () => {
    const held = holdLookup();
    apiFetch.mockImplementation(async (path: string) => {
      if (path.includes("/conversations/c1/steps/")) return held.promise;
      return answerOthers(path);
    });
    const { rerender } = render(
      <Transcript key="c1" items={[turn[0]!, wide]} running={false} conversationId="c1" />,
    );
    fireEvent.click(screen.getByRole("button", { name: /1 step/ }));
    fireEvent.click(screen.getByRole("button", { name: /Searched your mail/ }));
    fireEvent.click(screen.getByRole("button", { name: "and 7 more" }));

    // The sidebar keys the transcript by conversation, so the switch remounts it.
    rerender(<Transcript key="c2" items={[turn[0]!, wide]} running={false} conversationId="c2" />);
    expect(screen.getByRole("button", { name: /1 step/ })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /1 step/ }));
    fireEvent.click(screen.getByRole("button", { name: /Searched your mail/ }));
    expect(screen.getByRole("button", { name: "and 7 more" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );

    await act(async () => held.resolve({ rows: rowsOf(12), listed: 12, total: 12 }));
    expect(screen.queryByText("Mail 11")).toBeNull();
    expect(screen.getByRole("button", { name: "and 7 more" })).toBeVisible();
    expect(stepCalls().map(([path]) => path)).toEqual([
      "/api/v1/mail/assistant/conversations/c1/steps/a/rows?seq=2",
    ]);
  });
});
