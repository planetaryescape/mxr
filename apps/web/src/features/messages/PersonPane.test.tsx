/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, expect, test, vi } from "vitest";

import type { Conversation, MessagesRow, MessagesTopic, PersonPage } from "./api";
import { PersonPane, type PersonPaneProps } from "./PersonPane";

const http = vi.hoisted(() => ({ send: vi.fn<(path: string) => Promise<unknown>>() }));
vi.mock("@/api/client", async (original) => ({
  ...(await original<typeof import("@/api/client")>()),
  apiFetch: http.send,
}));
vi.mock("@/components/ModeFrame", () => ({
  ModeFrame: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));
vi.mock("@/components/PageFrame", () => ({
  PageFrame: ({ titleContent, title }: { titleContent?: ReactNode; title: string }) => (
    <header>
      <h1>{titleContent ?? title}</h1>
    </header>
  ),
}));
vi.mock("@/components/ReasonLine", () => ({ ReasonLine: () => null }));
vi.mock("@/components/ModeMark", () => ({ ModeMarks: () => null }));
vi.mock("@/features/modes/membership", () => ({
  otherModes: () => [],
  useThreadModes: () => ({ data: undefined }),
}));

const message = {
  message_id: "message-1",
  from: { name: "Samir Patel", email: "samir.p@home.example" },
  from_me: false,
  date: "2026-10-21T12:00:00Z",
  text: "Can you review this?",
  trimmed: { quote: false, signature: false },
  layout: "compact",
  paragraphs: 1,
} satisfies Conversation["messages"][number];

const conversation: Conversation = {
  account_id: "account",
  thread_id: "thread-1",
  subject: "Contract renewal",
  shape: "one_to_one",
  state: "your_turn",
  earlier_count: 0,
  messages: [message],
  composer: { label: "Reply", reply_to_message_id: message.message_id, reply_all: false },
};

const topic: MessagesTopic = {
  account_id: "account",
  thread_id: conversation.thread_id,
  subject: conversation.subject,
  shape: "one_to_one",
  state: "your_turn",
  last_at: message.date,
  message_count: 1,
  message_ids: [message.message_id],
  reply_to_message_id: message.message_id,
};

const row: MessagesRow = {
  id: "person:samir@launchpad.example",
  kind: "person",
  account_id: "account",
  band: "recent",
  title: "Samir Patel",
  person: {
    id: "samir@launchpad.example",
    name: "Samir Patel",
    addresses: ["samir@launchpad.example", "samir.p@home.example"],
  },
  members: [],
  closeness: "regular",
  your_turn: true,
  last_at: message.date,
  topics: [topic],
  overdue: false,
  pinned: false,
  unread: false,
  why: "",
};

const page: PersonPage = {
  row,
  topics: [topic],
  conversation,
  header_line: "Last: today",
  relationship_line: "Known correspondent",
};

function props(loading: boolean, selectedTopic: string | null): PersonPaneProps {
  return {
    page,
    topic: selectedTopic,
    left: 1,
    hidden: new Set(),
    arrival: null,
    loading,
    asSent: null,
    replyAll: false,
    pending: null,
    ackLoading: false,
    onBack: () => {},
    onTopic: () => {},
    onToggleAsSent: () => {},
    onReplyAllChange: () => {},
    onReply: () => {},
    onGotIt: () => {},
    onGotItFocus: () => {},
    onUndoGotIt: () => {},
    onDone: () => {},
    onTodo: () => {},
  };
}

afterEach(() => {
  http.send.mockReset();
});

test("placeholder account and topic changes hide and close old disclosures", () => {
  http.send.mockReturnValue(new Promise(() => {}));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const initial = props(false, "thread-1");
  const view = render(
    <QueryClientProvider client={client}>
      <PersonPane {...initial} />
    </QueryClientProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: /Message info for Samir Patel/ }));
  expect(screen.getByRole("dialog", { name: "Message info" })).toBeInTheDocument();

  view.rerender(
    <QueryClientProvider client={client}>
      <PersonPane {...props(true, "thread-1")} />
    </QueryClientProvider>,
  );

  expect(screen.queryByRole("dialog", { name: "Message info" })).toBeNull();
  expect(screen.queryByRole("button", { name: /Message info/ })).toBeNull();
  expect(screen.queryByRole("button", { name: /Show email addresses/ })).toBeNull();

  view.rerender(
    <QueryClientProvider client={client}>
      <PersonPane {...props(true, "thread-2")} />
    </QueryClientProvider>,
  );
  expect(screen.queryByRole("dialog", { name: "Message info" })).toBeNull();
});

test("group person pages keep their plain text heading", () => {
  const groupRow: MessagesRow = {
    ...row,
    id: "group:thread-1",
    kind: "group",
    title: "Samir, Ruth",
    person: null,
    members: [],
  };
  render(
    <QueryClientProvider client={new QueryClient()}>
      <PersonPane {...props(false, "thread-1")} page={{ ...page, row: groupRow }} />
    </QueryClientProvider>,
  );

  expect(screen.getByRole("heading", { name: "Samir, Ruth" })).toBeInTheDocument();
});
