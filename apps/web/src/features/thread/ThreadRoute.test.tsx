/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { ThreadRoute } from "./ThreadRoute";
import { RightRail } from "@/components/RightRail";
import type { ThreadResponse } from "@/features/mailbox/types";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useModals } from "@/state/modalStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  pathname: "/m/inbox/thread-1",
}));

const api = vi.hoisted(() => ({
  fetchShell: vi.fn<() => Promise<unknown>>(),
  fetchThread: vi.fn<(threadId: string) => Promise<ThreadResponse>>(),
  fetchSenderProfile: vi.fn<() => Promise<unknown>>(),
  modifyLabels:
    vi.fn<(messageIds: string[], add: string[], remove: string[]) => Promise<unknown>>(),
  summarizeThread: vi.fn<() => Promise<unknown>>(),
}));

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useNavigate: () => router.navigate,
    useRouterState: ({
      select,
    }: {
      select: (state: { location: { pathname: string } }) => unknown;
    }) => select({ location: { pathname: router.pathname } }),
  };
});

vi.mock("@/features/mailbox/MailboxRoute", () => ({
  MailboxRoute: () => null,
}));

vi.mock("@/features/mailbox/SnoozeDialog", () => ({
  SnoozeDialog: () => null,
}));

vi.mock("@/features/mailbox/api", () => ({
  archiveMessages: vi.fn<(messageIds: string[]) => Promise<unknown>>(),
  fetchSenderProfile: api.fetchSenderProfile,
  fetchShell: api.fetchShell,
  fetchThread: api.fetchThread,
  markReadMessages: vi.fn<(messageIds: string[], read: boolean) => Promise<unknown>>(),
  modifyLabels: api.modifyLabels,
  shellKey: ["shell"],
  spamMessages: vi.fn<(messageIds: string[]) => Promise<unknown>>(),
  starMessages: vi.fn<(messageIds: string[], starred: boolean) => Promise<unknown>>(),
  summarizeThread: api.summarizeThread,
  trashMessages: vi.fn<(messageIds: string[]) => Promise<unknown>>(),
  undoMutation: vi.fn<(mutationId: string) => Promise<unknown>>(),
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn<(message: string, options?: unknown) => void>(),
    success: vi.fn<(message: string, options?: unknown) => void>(),
  },
}));

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

const thread: ThreadResponse = {
  thread: {
    account_id: "account-1",
    id: "thread-1",
    latest_date: "2026-05-11T10:00:00Z",
    message_count: 2,
    participants: [{ email: "sender@example.com", name: "Sender" }],
    snippet: "hello",
    subject: "Label workflow",
    unread_count: 0,
  },
  messages: [
    {
      id: "msg-1",
      kind: "message",
      thread_id: "thread-1",
      provider_id: "provider-msg-1",
      sender: "Sender",
      subject: "Label workflow",
      snippet: "hello",
      date: "2026-05-11T10:00:00Z",
      date_label: "May 11",
      date_full: "May 11, 2026, 10:00 AM",
      date_relative: "now",
      labels: [{ id: "label-work", name: "Work", kind: "user" }],
      unread: false,
      starred: false,
      has_attachments: false,
    },
    {
      id: "msg-2",
      kind: "message",
      thread_id: "thread-1",
      provider_id: "provider-msg-2",
      sender: "Sender",
      subject: "Label workflow",
      snippet: "follow-up",
      date: "2026-05-11T10:01:00Z",
      date_label: "May 11",
      date_full: "May 11, 2026, 10:01 AM",
      date_relative: "now",
      labels: [{ id: "label-work", name: "Work", kind: "user" }],
      unread: false,
      starred: false,
      has_attachments: false,
    },
  ],
  bodies: [
    {
      message_id: "msg-1",
      text_plain: "hello",
      attachments: [
        {
          id: "attachment-1",
          filename: "agenda.pdf",
          mime_type: "application/pdf",
          size_bytes: 1024,
        },
      ],
    },
  ],
  right_rail: { title: "Thread context", items: ["2 messages", "1 attachment"] },
};

describe("ThreadRoute", () => {
  beforeEach(() => {
    useMailboxPane.setState({ activePane: "reader", sidebarIndex: 0 });
    api.fetchThread.mockResolvedValue(thread);
    api.fetchShell.mockResolvedValue({
      shell: {},
      sidebar: {
        sections: [
          {
            id: "labels",
            title: "Labels",
            items: [
              {
                id: "work",
                label: "Work",
                lens: { kind: "label", labelId: "label-work" },
              },
              {
                id: "later",
                label: "Later",
                lens: { kind: "label", labelId: "label-later" },
              },
            ],
          },
        ],
      },
    });
    api.modifyLabels.mockResolvedValue({ ok: true, result: { succeeded: 2 } });
  });

  afterEach(() => {
    vi.clearAllMocks();
    useMailboxPane.setState({ activePane: "mailbox", sidebarIndex: 0 });
    useModals.setState({ rightRail: null });
  });

  test("opens label editing from the reader keyboard shortcut and saves add/remove changes", async () => {
    renderWithQueryClient(<ThreadRoute />);

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    fireEvent.keyDown(window, { key: "l" });

    const work = await screen.findByRole("checkbox", { name: "Work" });
    const later = screen.getByRole("checkbox", { name: "Later" });
    expect(work).toBeChecked();
    expect(later).not.toBeChecked();

    fireEvent.click(work);
    fireEvent.click(later);
    fireEvent.click(screen.getByRole("button", { name: /apply label changes/i }));

    await waitFor(() => {
      expect(api.modifyLabels).toHaveBeenCalledWith(["msg-1", "msg-2"], ["Later"], ["Work"]);
    });
  });

  test("opens thread context in the right rail from the reader keyboard shortcut", async () => {
    renderWithQueryClient(
      <>
        <ThreadRoute />
        <RightRail />
      </>,
    );

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    fireEvent.keyDown(window, { key: "L" });

    expect(await screen.findByRole("heading", { name: "Thread context" })).toBeVisible();
    expect(screen.getByText("1 attachment")).toBeVisible();
  });

  test("opens attachments in the right rail from the reader keyboard shortcut", async () => {
    renderWithQueryClient(
      <>
        <ThreadRoute />
        <RightRail />
      </>,
    );

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    fireEvent.keyDown(window, { key: "A" });

    expect(await screen.findByText("attachments")).toBeVisible();
  });
});
