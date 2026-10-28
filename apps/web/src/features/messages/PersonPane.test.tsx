/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode } from "react";
import type { ReactNode } from "react";
import { afterEach, expect, test, vi } from "vitest";

import { useComposeUi } from "@/features/compose/composeUiStore";

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
  vi.unstubAllGlobals();
  http.send.mockReset();
  useComposeUi.setState(useComposeUi.getInitialState(), true);
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

test("keeps topic navigation compact and returns focus after selecting a group topic", async () => {
  const groupTopic: MessagesTopic = {
    ...topic,
    thread_id: "thread-2",
    subject: "Planning",
    shape: "group",
    with: ["Ruth"],
  };
  const onTopic = vi.fn<(threadId: string) => void>();
  const triggerName = /Choose conversation, Contract renewal, direct, 1 of 2/;
  render(
    <QueryClientProvider client={new QueryClient()}>
      <PersonPane
        {...props(false, "thread-1")}
        page={{ ...page, topics: [topic, groupTopic] }}
        onTopic={onTopic}
      />
    </QueryClientProvider>,
  );

  const trigger = screen.getByRole("button", { name: triggerName });
  expect(screen.queryByTestId("topics")).toBeNull();
  fireEvent.click(trigger);
  expect(screen.getByTestId("topics")).toBeInTheDocument();
  const group = screen.getByRole("button", { name: /with Ruth: Planning/ });
  fireEvent.click(group);
  expect(onTopic).toHaveBeenCalledWith("thread-2");
  expect(screen.queryByTestId("topics")).toBeNull();
  await waitFor(() => expect(trigger).toHaveFocus());
  expect(trigger).toHaveTextContent("Contract renewal");
});

test("Escape closes the topic list and restores focus to its trigger", async () => {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <PersonPane {...props(false, "thread-1")} page={{ ...page, topics: [topic] }} />
    </QueryClientProvider>,
  );
  const trigger = screen.getByTestId("topics-trigger");
  fireEvent.click(trigger);
  fireEvent.keyDown(screen.getByTestId("topics"), { key: "Escape" });
  expect(screen.queryByTestId("topics")).toBeNull();
  await waitFor(() => expect(trigger).toHaveFocus());
});

test("shows the selected reply mode as an explicit action", () => {
  const onReplyAllChange = vi.fn<(enabled: boolean) => void>();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <PersonPane {...props(false, "thread-1")} onReplyAllChange={onReplyAllChange} />
    </QueryClientProvider>,
  );

  const modes = within(screen.getByRole("group", { name: "Reply mode" }));
  const reply = modes.getByRole("button", { name: "Reply" });
  const replyAll = modes.getByRole("button", { name: "Reply all" });
  expect(reply).toHaveAttribute("aria-pressed", "true");
  expect(replyAll).toHaveAttribute("aria-pressed", "false");
  fireEvent.click(replyAll);
  expect(onReplyAllChange).toHaveBeenCalledWith(true);
});

test("keeps this topic replyable while a different topic's reply is docked", () => {
  useComposeUi.setState({
    intent: {
      key: "compose:reply:m1",
      title: "Reply",
      kind: "reply",
      messageId: "m1",
      contextThreadId: "thread-1",
      contextTitle: "Contract renewal",
    },
    surface: "inline",
  });
  const nextConversation: Conversation = {
    ...conversation,
    thread_id: "thread-2",
    subject: "Launch checklist",
    messages: [{ ...message, message_id: "message-2" }],
    composer: { label: "Reply", reply_to_message_id: "message-2", reply_all: false },
  };
  render(
    <QueryClientProvider client={new QueryClient()}>
      <PersonPane
        {...props(false, "thread-2")}
        page={{
          ...page,
          conversation: nextConversation,
          topics: [{ ...topic, thread_id: "thread-2", subject: "Launch checklist" }],
        }}
      />
    </QueryClientProvider>,
  );

  expect(screen.getByTestId("composer")).toBeInTheDocument();
  expect(screen.getByRole("group", { name: "Reply mode" })).toBeInTheDocument();
});

test("positions the latest message start once, including after StrictMode cancels an effect", async () => {
  const callbacks = new Map<number, FrameRequestCallback>();
  const cancelled: number[] = [];
  let nextFrame = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    const id = ++nextFrame;
    callbacks.set(id, callback);
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    cancelled.push(id);
    callbacks.delete(id);
  });
  const view = render(
    <StrictMode>
      <QueryClientProvider client={new QueryClient()}>
        <PersonPane {...props(false, "thread-1")} />
      </QueryClientProvider>
    </StrictMode>,
  );
  const area = screen.getByTestId("conversation-reading-area");
  const latest = area.querySelector<HTMLElement>('[data-latest="true"]');
  if (!latest) throw new Error("The conversation has no latest message element");
  vi.spyOn(area, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 100, 400, 700));
  vi.spyOn(latest, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 700, 400, 250));
  expect(cancelled.length).toBeGreaterThan(0);
  const pendingFrames = [...callbacks.values()];
  callbacks.clear();
  for (const callback of pendingFrames) callback(0);
  expect(area.scrollTop).toBe(600);

  area.scrollTop = 850;
  view.rerender(
    <StrictMode>
      <QueryClientProvider client={new QueryClient()}>
        <PersonPane {...props(false, "thread-1")} page={{ ...page, header_line: "Updated" }} />
      </QueryClientProvider>
    </StrictMode>,
  );
  expect(callbacks.size).toBe(0);
  expect(area.scrollTop).toBe(850);
});
