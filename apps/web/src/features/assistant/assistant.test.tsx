/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render as renderPlain,
  screen,
  within,
  type RenderOptions,
} from "@testing-library/react";
import type { ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { AssistantDecision, AssistantItem, AssistantPending } from "./api";
import { renderMarkdown } from "./markdown";
import { PendingCards } from "./PendingCards";
import { Transcript, mutationIdOf } from "./Transcript";
import { irreversibleWarning } from "./wire";
import { LiveLine } from "./LiveLine";

const navigate = vi.fn<(options: { href: string }) => Promise<void>>();
vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  useNavigate: () => navigate,
}));
const invalidate = vi.fn<() => Promise<void>>();
vi.mock("@/features/mail-actions/mailQueryInvalidation", () => ({
  invalidateMailQueries: () => invalidate(),
}));

const apiFetch = vi.hoisted(() => vi.fn<(path: string, init?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch,
}));
const answerLookups = (threads: unknown[]) =>
  apiFetch.mockResolvedValue({ kind: "ModeMembership", threads });

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
  test("has one primary button, Apply, and it approves", () => {
    const onDecide = cards([approval]);
    const apply = screen.getByRole("button", { name: /^Apply/ });
    expect(apply).toHaveAttribute("data-slot", "button");
    const filled = screen
      .getAllByRole("button")
      .filter((button) => button.className.includes("bg-primary"));
    expect(filled).toEqual([apply]);
    fireEvent.click(apply);
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
});

describe("question", () => {
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

  describe("email rows", () => {
    const replyWith = (rows: unknown[]) => ({ ...turn[2]!, rows }) as AssistantItem;
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
});

describe("live line", () => {
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
