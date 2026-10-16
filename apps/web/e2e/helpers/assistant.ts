import type { Page } from "@playwright/test";

/*
 * A recorded assistant conversation, served from fixtures: the design gate
 * looks at the panel in each state without a model in the loop. The shapes
 * are the daemon's (src/api/generated.ts), W7a fields included.
 */

type Json = Record<string, unknown>;

const CONVERSATION_ID = "copy-desk";

const user: Json = {
  kind: "user",
  seq: 1,
  text: "Search for CI failure emails and archive them",
  created_at: 1_760_000_000,
};

const searchStep = (state: string): Json => ({
  kind: "tool_call",
  seq: 2,
  call_id: "search",
  tool: "mxr_search",
  reason: "Find the CI failure emails.",
  title: {
    running: "Searching your mail for “CI failure”",
    done: "Searched your mail: 14 found",
  },
  arguments: { query: "CI failure", limit: 25 },
  state,
  result: '{"kind":"SearchResults","total":14}',
  readout: {
    total: 14,
    rows: [
      {
        title: "CI failed on main: build-linux",
        detail: "GitHub Actions",
        link: { kind: "thread", thread_id: "t-ci-1" },
      },
      {
        title: "CI failed on feat/web: e2e",
        detail: "GitHub Actions",
        link: { kind: "thread", thread_id: "t-ci-2" },
      },
      {
        title: "Nightly pipeline failed",
        detail: "Buildkite",
        link: { kind: "thread", thread_id: "t-ci-3" },
      },
    ],
  },
  created_at: 1_760_000_001,
});

const archiveStep = (state: string): Json => ({
  kind: "tool_call",
  seq: 3,
  call_id: "archive",
  tool: "mxr_mutate",
  reason: "Archive the CI failure emails.",
  title: {
    running: "Archiving 14 emails",
    done: state === "waiting" ? "Waiting for you: archive 14 emails" : "Archived 14 emails",
  },
  arguments: { action: "archive", message_ids: ["m1", "m2", "m3"], confirm: true },
  state,
  result: state === "approved" ? '{"mutation_id":"mut-1","succeeded":14}' : null,
  created_at: 1_760_000_002,
});

const reply: Json = {
  kind: "assistant",
  seq: 4,
  text: "I found 14 CI failure emails. Most are from [GitHub Actions](mxr:person/person:ci@github.example); the newest is [CI failed on main: build-linux](mxr:thread/t-ci-1). None of them needs a reply.",
  model: "gemma4",
  rows: [
    {
      title: "CI failed on main: build-linux",
      detail: "GitHub Actions",
      link: { kind: "thread", thread_id: "t-ci-1" },
    },
    {
      title: "CI failed on feat/web: e2e",
      detail: "GitHub Actions",
      link: { kind: "thread", thread_id: "t-ci-2" },
    },
    {
      title: "Nightly pipeline failed",
      detail: "Buildkite",
      link: { kind: "thread", thread_id: "t-ci-3" },
    },
  ],
  created_at: 1_760_000_003,
};

const proofPending: Json = {
  kind: "approval",
  call_id: "archive",
  tool: "mxr_mutate",
  title: "Archive 14 emails",
  reason: "They are all build notifications older than a day, and none is waiting on you.",
  arguments: { action: "archive", message_ids: ["m1", "m2", "m3"], confirm: true },
  preview: {
    preview: {
      kind: "Envelopes",
      envelopes: [
        {
          id: "m1",
          subject: "CI failed on main: build-linux",
          from: { name: "GitHub Actions", email: "ci@github.example" },
          date: "2026-10-14T09:00:00Z",
        },
        {
          id: "m2",
          subject: "CI failed on feat/web: e2e",
          from: { name: "GitHub Actions", email: "ci@github.example" },
          date: "2026-10-13T16:20:00Z",
        },
        {
          id: "m3",
          subject: "Nightly pipeline failed",
          from: { name: "Buildkite", email: "no-reply@buildkite.example" },
          date: "2026-10-12T03:05:00Z",
        },
      ],
    },
  },
};

const questionPending: Json = {
  kind: "question",
  call_id: "ask",
  reason: "Two accounts match.",
  questions: [
    {
      header: "Account",
      question: "Which account should I tidy?",
      options: [
        { label: "Work (Recommended)", description: "ann@work.example, 2,310 emails" },
        { label: "Home", description: "ann@home.example, 480 emails" },
      ],
      multi_select: false,
      allow_other: true,
    },
  ],
};

const base = (items: Json[], extra: Json = {}): Json => ({
  id: CONVERSATION_ID,
  title: "CI failure emails",
  status: "idle",
  items,
  pending: [],
  auto_approve: false,
  created_at: 1_760_000_000,
  updated_at: 1_760_000_010,
  ...extra,
});

export const ASSISTANT_STATES = {
  empty: null,
  running: base([user, searchStep("running")], { status: "running" }),
  finished: base([user, searchStep("done"), archiveStep("approved"), reply]),
  proof: base([user, searchStep("done"), archiveStep("waiting")], {
    status: "awaiting_user",
    pending: [proofPending],
  }),
  question: base(
    [
      user,
      {
        kind: "tool_call",
        seq: 2,
        call_id: "ask",
        tool: "ask_user",
        reason: "",
        title: { running: "Asking you a question", done: "Waiting for you: a question" },
        arguments: {},
        state: "waiting",
        created_at: 1_760_000_001,
      },
    ],
    { status: "awaiting_user", pending: [questionPending] },
  ),
  failure: base(
    [
      user,
      searchStep("done"),
      {
        ...archiveStep("failed"),
        title: {
          running: "Archiving 14 emails",
          done: "Couldn’t archive 14 emails: they were already moved",
        },
      },
      { ...reply, failed_call_ids: ["archive"], rows: [] },
    ],
    {
      last_outcome: "failed",
      last_error:
        "The model stopped answering. Send your message again, or switch models in Settings.",
      last_error_detail: "LLM returned an empty completion",
    },
  ),
} satisfies Record<string, Json | null>;

export type AssistantState = keyof typeof ASSISTANT_STATES;

/** Opens the app with the panel open on a recorded conversation (or none), and answers `decide`. */
export async function mockAssistant(
  page: Page,
  state: AssistantState,
  onDecide?: (body: unknown) => void,
): Promise<void> {
  const conversation = ASSISTANT_STATES[state];
  await page.addInitScript(
    ({ id }) => {
      localStorage.setItem(
        "mxr:assistant",
        JSON.stringify({ state: { open: true, conversationId: id }, version: 0 }),
      );
    },
    { id: conversation ? CONVERSATION_ID : null },
  );
  await page.route("**/api/v1/mail/assistant/**", async (route) => {
    const url = new URL(route.request().url());
    const json = (body: unknown) => route.fulfill({ json: body });
    if (url.pathname.endsWith("/availability")) {
      return json({ availability: { available: true, leaves_machine: false, model: "gemma4" } });
    }
    if (url.pathname.endsWith("/decide")) {
      onDecide?.(route.request().postDataJSON());
      return json({ conversation: ASSISTANT_STATES.finished });
    }
    if (url.pathname.endsWith("/conversations")) {
      return json({
        conversations: conversation
          ? [
              {
                id: CONVERSATION_ID,
                title: "CI failure emails",
                status: (conversation as Json).status,
                pending_count: ((conversation as Json).pending as unknown[]).length,
                message_count: 4,
                created_at: 1_760_000_000,
                updated_at: 1_760_000_010,
              },
            ]
          : [],
      });
    }
    return json({ conversation });
  });
}
