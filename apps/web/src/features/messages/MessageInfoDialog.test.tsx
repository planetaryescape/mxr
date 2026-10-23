/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import type { ThreadResponse } from "@/features/mailbox/types";

import { CopyableAddress } from "./AddressDisclosure";
import { MessageInfoDialog } from "./MessageInfoDialog";
import { PersonIdentity } from "./PersonIdentity";
import type { ConversationMessage, MessagesRow } from "./api";

const http = vi.hoisted(() => ({ send: vi.fn<(path: string) => Promise<unknown>>() }));
vi.mock("@/api/client", async (original) => ({
  ...(await original<typeof import("@/api/client")>()),
  apiFetch: http.send,
}));
const clipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard");

const from = { name: "Samir Patel", email: "samir.p@home.example" };
const first: ConversationMessage = {
  message_id: "message-1",
  from,
  from_me: false,
  date: "2026-10-20T12:00:00Z",
  text: "First message",
  trimmed: { quote: false, signature: false },
  layout: "compact",
  paragraphs: 1,
};
const second: ConversationMessage = {
  ...first,
  message_id: "message-2",
  date: "2026-10-21T12:00:00Z",
  text: "Second message",
};

function threadResponse(): ThreadResponse {
  return {
    thread: {
      account_id: "account",
      id: "thread-1",
      latest_date: "2026-10-21T12:00:00Z",
      message_count: 2,
      participants: [],
      snippet: "Second message",
      subject: "Different audiences",
      unread_count: 0,
    },
    messages: [
      {
        id: "message-1",
        kind: "message",
        thread_id: "thread-1",
        provider_id: "p1",
        sender: "Samir Patel",
        subject: "Different audiences",
        snippet: "First message",
        date: first.date,
        date_label: "Yesterday",
        date_full: "Tuesday",
        date_relative: "1d",
        to: [{ name: "Alex", email: "alex@example.com" }],
        cc: [{ name: "Jo", email: "jo@example.com" }],
        bcc: [{ name: "Lee", email: "lee@example.com" }],
        unread: false,
        starred: false,
        has_attachments: false,
      },
      {
        id: "message-2",
        kind: "message",
        thread_id: "thread-1",
        provider_id: "p2",
        sender: "Samir Patel",
        subject: "Different audiences",
        snippet: "Second message",
        date: second.date,
        date_label: "Today",
        date_full: "Wednesday",
        date_relative: "now",
        to: [{ name: "Maya", email: "maya@example.com" }],
        unread: false,
        starred: false,
        has_attachments: false,
      },
    ],
    bodies: [],
  };
}

function queryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: Infinity } },
  });
}

function renderDialog(message = first, client = queryClient()) {
  const result = render(
    <QueryClientProvider client={client}>
      <MessageInfoDialog threadId="thread-1" message={message} />
    </QueryClientProvider>,
  );
  return { ...result, client };
}

afterEach(() => {
  http.send.mockReset();
  vi.unstubAllGlobals();
  if (clipboardDescriptor) {
    Object.defineProperty(navigator, "clipboard", clipboardDescriptor);
  } else {
    Reflect.deleteProperty(navigator, "clipboard");
  }
});

test("shows the exact recorded audience for each message, including available Bcc", async () => {
  const response = threadResponse();
  expect(response.messages[1]).not.toHaveProperty("cc");
  expect(response.messages[1]).not.toHaveProperty("bcc");
  http.send.mockResolvedValue(response);
  const client = queryClient();
  const view = render(
    <QueryClientProvider client={client}>
      <MessageInfoDialog threadId="thread-1" message={first} />
    </QueryClientProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: /Message info for Samir Patel/ }));
  expect(await screen.findByText("alex@example.com")).toBeInTheDocument();
  expect(screen.getByText("jo@example.com")).toBeInTheDocument();
  expect(screen.getByText("lee@example.com")).toBeInTheDocument();
  expect(screen.getByText("samir.p@home.example")).toBeInTheDocument();

  view.rerender(
    <QueryClientProvider client={client}>
      <MessageInfoDialog threadId="thread-1" message={second} />
    </QueryClientProvider>,
  );
  expect(await screen.findByText("maya@example.com")).toBeInTheDocument();
  expect(screen.queryByText("alex@example.com")).toBeNull();
  expect(screen.getByRole("region", { name: "Cc" })).toHaveTextContent("None recorded");
  expect(screen.getByRole("region", { name: "Bcc" })).toHaveTextContent(
    "Not recorded in this copy",
  );
});

test("shows unavailable metadata and offers recovery after a query error", async () => {
  http.send.mockRejectedValueOnce(new Error("offline")).mockResolvedValue(threadResponse());
  renderDialog();
  fireEvent.click(screen.getByRole("button", { name: /Message info for Samir Patel/ }));
  expect(await screen.findByText("Couldn't load message info.")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("alex@example.com")).toBeInTheDocument();
  expect(http.send).toHaveBeenCalledTimes(2);
});

test("treats explicitly empty recipient arrays as empty recorded lists", async () => {
  const response = threadResponse();
  response.messages[0]!.to = [];
  response.messages[0]!.cc = [];
  response.messages[0]!.bcc = [];
  http.send.mockResolvedValue(response);
  renderDialog();
  fireEvent.click(screen.getByRole("button", { name: /Message info for Samir Patel/ }));

  expect(await screen.findByRole("region", { name: "To" })).toHaveTextContent("None recorded");
  expect(screen.getByRole("region", { name: "Cc" })).toHaveTextContent("None recorded");
  expect(screen.getByRole("region", { name: "Bcc" })).toHaveTextContent(
    "Not recorded in this copy",
  );
  expect(screen.queryByText("Unavailable")).toBeNull();
  expect(screen.getByText(/does not confirm that nobody was hidden/)).toBeInTheDocument();
});

test("shows unavailable when a nonempty recipient list has no usable address", async () => {
  const response = threadResponse();
  response.messages[0]!.cc = [{ email: "   " }];
  http.send.mockResolvedValue(response);
  renderDialog();
  fireEvent.click(screen.getByRole("button", { name: /Message info for Samir Patel/ }));

  expect(await screen.findByRole("region", { name: "Cc" })).toHaveTextContent("Unavailable");
});

test("Escape closes message info and returns focus to its trigger", async () => {
  http.send.mockResolvedValue(threadResponse());
  render(
    <QueryClientProvider client={queryClient()}>
      <MessageInfoDialog threadId="thread-1" message={first} />
    </QueryClientProvider>,
  );
  const trigger = screen.getByRole("button", { name: /Message info for Samir Patel/ });
  fireEvent.click(trigger);
  const dialog = await screen.findByRole("dialog", { name: "Message info" });
  fireEvent.keyDown(dialog, { key: "Escape" });
  await waitFor(() => expect(trigger).toHaveFocus());
});

test("switching topics does not reuse the previous thread's audience", async () => {
  let resolveThread!: (response: ThreadResponse) => void;
  const pendingThread = new Promise<ThreadResponse>((resolve) => {
    resolveThread = resolve;
  });
  const otherThread = threadResponse();
  otherThread.thread.id = "thread-2";
  otherThread.messages = [
    {
      ...otherThread.messages[0]!,
      thread_id: "thread-2",
      to: [{ email: "next-topic@example.com" }],
      cc: [],
      bcc: [],
    },
  ];
  http.send.mockResolvedValueOnce(threadResponse()).mockReturnValueOnce(pendingThread);
  const client = queryClient();
  const view = render(
    <QueryClientProvider client={client}>
      <MessageInfoDialog threadId="thread-1" message={first} />
    </QueryClientProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: /Message info for Samir Patel/ }));
  expect(await screen.findByText("alex@example.com")).toBeInTheDocument();
  view.rerender(
    <QueryClientProvider client={client}>
      <MessageInfoDialog threadId="thread-2" message={first} />
    </QueryClientProvider>,
  );
  expect(await screen.findByText("Loading message info…")).toBeInTheDocument();
  expect(screen.queryByText("alex@example.com")).toBeNull();
  resolveThread(otherThread);
  expect(await screen.findByText("next-topic@example.com")).toBeInTheDocument();
});

test("shows every linked person address separately from the original sender", async () => {
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
    your_turn: false,
    last_at: "2026-10-21T12:00:00Z",
    topics: [],
    overdue: false,
    pinned: false,
    unread: false,
    why: "",
  };
  render(<PersonIdentity row={row} />);
  fireEvent.click(screen.getByRole("button", { name: "Show email addresses for Samir Patel" }));
  expect(await screen.findByText("samir@launchpad.example")).toBeInTheDocument();
  expect(screen.getByText("samir.p@home.example")).toBeInTheDocument();
  const trigger = screen.getByRole("button", { name: "Show email addresses for Samir Patel" });
  fireEvent.keyDown(screen.getByRole("dialog", { name: "Samir Patel email addresses" }), {
    key: "Escape",
  });
  await waitFor(() => expect(trigger).toHaveFocus());
});

test("selects the address when clipboard access fails", async () => {
  const writeText = vi.fn<(text: string) => Promise<void>>().mockRejectedValue(new Error("denied"));
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
  render(<CopyableAddress address="samir@launchpad.example" />);
  fireEvent.click(screen.getByRole("button", { name: "Copy samir@launchpad.example" }));
  await waitFor(() => {
    expect(window.getSelection()?.toString()).toBe("samir@launchpad.example");
  });
  expect(writeText).toHaveBeenCalledWith("samir@launchpad.example");
});
