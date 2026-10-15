/* @vitest-environment jsdom */

import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { AssistantDecision, AssistantItem, AssistantPending } from "./api";
import { describeChange, previewMessages } from "./describe";
import { renderMarkdown } from "./markdown";
import { PendingCards } from "./PendingCards";
import { Transcript, inputWords } from "./Transcript";

const navigate = vi.fn<(options: { href: string }) => Promise<void>>();
vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  useNavigate: () => navigate,
}));

const approval: AssistantPending = {
  kind: "approval",
  call_id: "c1",
  tool: "mxr_mutate",
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
      options: [{ label: "Work" }, { label: "Home", description: "Personal Gmail" }],
      multi_select: false,
      allow_other: true,
    },
  ],
};

function cards(pending: AssistantPending[]) {
  const onDecide = vi.fn<(decision: AssistantDecision) => void>();
  render(<PendingCards pending={pending} busy={false} onDecide={onDecide} />);
  return onDecide;
}

describe("approval card", () => {
  test("names the change and lists the messages it touches", () => {
    const onDecide = cards([approval]);
    expect(screen.getByRole("heading", { name: "Archive 2 messages" })).toBeVisible();
    expect(screen.getByText("Your parcel shipped")).toBeVisible();
    expect(screen.getByText("Out for delivery")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(onDecide).toHaveBeenCalledWith({
      call_id: "c1",
      decision: "approve",
    });
  });

  test("a rejection carries the user's reason", () => {
    const onDecide = cards([approval]);
    fireEvent.click(screen.getByRole("button", { name: "Reject" }));
    fireEvent.change(screen.getByLabelText("Why, or what to do instead"), {
      target: { value: "keep the second one" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Reject" }));
    expect(onDecide).toHaveBeenCalledWith({
      call_id: "c1",
      decision: "reject",
      reason: "keep the second one",
    });
  });
});

describe("question card", () => {
  test("answers with the chosen option, and only once something is chosen", () => {
    const onDecide = cards([question]);
    const answer = screen.getByRole("button", { name: "Answer" });
    expect(answer).toBeDisabled();
    const group = screen.getByRole("radiogroup", { name: "Which account?" });
    fireEvent.click(within(group).getByRole("radio", { name: /Home/ }));
    expect(within(group).getByRole("radio", { name: /Home/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    fireEvent.click(answer);
    expect(onDecide).toHaveBeenCalledWith({
      call_id: "q1",
      decision: "answer",
      answers: [{ selected: ["Home"], other: undefined }],
    });
  });

  test("typing an answer replaces a single choice, and Skip declines", () => {
    const onDecide = cards([question]);
    fireEvent.click(screen.getByRole("radio", { name: /Work/ }));
    fireEvent.change(screen.getByLabelText("Your own answer"), {
      target: { value: "Both" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Answer" }));
    expect(onDecide).toHaveBeenLastCalledWith({
      call_id: "q1",
      decision: "answer",
      answers: [{ selected: [], other: "Both" }],
    });
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(onDecide).toHaveBeenLastCalledWith({
      call_id: "q1",
      decision: "reject",
    });
  });
});

describe("transcript", () => {
  const items: AssistantItem[] = [
    { kind: "user", seq: 1, text: "tidy shipping mail", created_at: 1 },
    {
      kind: "tool_call",
      seq: 2,
      call_id: "a",
      tool: "mxr_search",
      reason: "Find shipping notices.",
      arguments: { query: "shipped" },
      state: "done",
      result: '{"total":2}',
      created_at: 2,
    },
    { kind: "assistant", seq: 3, text: "Found **2**.", created_at: 3 },
  ];

  test("folds a finished turn's steps and keeps them one click away", () => {
    render(<Transcript items={items} running={false} />);
    expect(screen.getByText("tidy shipping mail")).toBeVisible();
    expect(screen.queryByText("Find shipping notices.")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /1 step/ }));
    expect(screen.getByText("Find shipping notices.")).toBeVisible();
    expect(screen.getByText("2").tagName).toBe("STRONG");
  });

  test("a folded turn counts its failed steps beside the reply", () => {
    const failed: AssistantItem = {
      kind: "tool_call",
      seq: 4,
      call_id: "b",
      tool: "mxr_mutate",
      reason: "Star it.",
      arguments: { action: "star", message_ids: ["m1"] },
      state: "failed",
      result: '{"blocked":true}',
      created_at: 4,
    };
    render(<Transcript items={[...items.slice(0, 2), failed, items[2]!]} running={false} />);
    expect(screen.getByRole("button", { name: /2 steps.*1 failed/ })).toBeVisible();
  });

  test("shows steps as they happen while the turn runs", () => {
    render(<Transcript items={items.slice(0, 2)} running />);
    expect(screen.getByText("Find shipping notices.")).toBeVisible();
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
    expect(html).toContain("me");
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

  test("clicking a reply link navigates inside the app", () => {
    navigate.mockClear();
    render(
      <Transcript
        running={false}
        items={[
          {
            kind: "assistant",
            seq: 1,
            text: "See [Q1 report](mxr:thread/t-1234567).",
            created_at: 0,
          },
        ]}
      />,
    );
    fireEvent.click(screen.getByRole("link", { name: "Q1 report" }));
    expect(navigate).toHaveBeenCalledWith({ href: "/m/archive/t-1234567" });
  });

  test("an opened step reads out what it found, each row a link", () => {
    navigate.mockClear();
    render(
      <Transcript
        running={false}
        items={[
          {
            kind: "tool_call",
            seq: 1,
            call_id: "c1",
            tool: "mxr_messages",
            reason: "To see who is waiting on you.",
            arguments: {},
            state: "done",
            result: '{"kind":"Messages"}',
            readout: {
              total: 9,
              rows: [
                {
                  title: "Nigel Nk",
                  detail: "German visa application pack",
                  link: { kind: "person", person_id: "person:n@x.com" },
                },
              ],
            },
            created_at: 0,
          },
        ]}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /1 step/ }));
    fireEvent.click(screen.getByRole("button", { name: /who is waiting on you/ }));
    expect(screen.getByText("Found 9")).toBeVisible();
    expect(screen.getByText("and 8 more")).toBeVisible();
    expect(screen.queryByText("Looked for")).toBeNull();
    fireEvent.click(screen.getByRole("link", { name: /Nigel Nk/ }));
    expect(navigate).toHaveBeenCalledWith({ href: "/messages?person=person%3An%40x.com" });
  });

  test("a call's inputs read as words, leaving out empty ones", () => {
    expect(inputWords({ query: "from:nigel", limit: 3, ids: ["a", "b"], cursor: null })).toEqual([
      "query “from:nigel”",
      "limit 3",
      "ids 2 items",
    ]);
    expect(inputWords({})).toEqual([]);
  });
});

describe("change titles and previews", () => {
  test("titles changes in words and finds a preview's messages", () => {
    expect(describeChange("mxr_mutate", { action: "trash", message_ids: ["a"] })).toBe(
      "Move 1 message to Trash",
    );
    expect(describeChange("mxr_send_draft", {})).toBe("Send a draft");
    expect(describeChange("mxr_unknown_thing", {})).toBe("Unknown thing");
    expect(previewMessages(approval.kind === "approval" ? approval.preview : null)).toHaveLength(2);
    expect(previewMessages({ nothing: true })).toEqual([]);
  });
});

describe("live and summarised turns", () => {
  test("the reply being written shows while the turn runs", () => {
    render(
      <Transcript
        items={[{ kind: "user", seq: 1, text: "hi", created_at: 1 }]}
        running
        writing="Looking at **your** mail"
      />,
    );
    expect(screen.getByText("your").tagName).toBe("STRONG");
  });

  test("a summary marks where earlier messages were folded, and auto-approved steps say so", () => {
    render(
      <Transcript
        items={[
          { kind: "user", seq: 1, text: "old", created_at: 1 },
          {
            kind: "summary",
            seq: 1,
            text: "They asked about invoices.",
            created_at: 2,
          },
          {
            kind: "tool_call",
            seq: 2,
            call_id: "a",
            tool: "mxr_mutate",
            reason: "Archive the receipts.",
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
    expect(screen.getByLabelText("Ran without asking")).toBeInTheDocument();
  });
});

describe("undo", () => {
  test("an approved mail change can be undone by its mutation id; reads can't", async () => {
    const { mutationIdOf } = await import("./Transcript");
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
