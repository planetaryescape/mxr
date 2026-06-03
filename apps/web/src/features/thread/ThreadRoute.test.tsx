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
import { useUiPrefs } from "@/state/uiPrefsStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  pathname: "/m/inbox/thread-1",
}));

const api = vi.hoisted(() => ({
  fetchShell: vi.fn<() => Promise<unknown>>(),
  fetchThread: vi.fn<(threadId: string) => Promise<ThreadResponse>>(),
  fetchSenderProfile: vi.fn<(input: { accountId: string; email: string }) => Promise<unknown>>(),
  listCommitments:
    vi.fn<
      (input: {
        accountId: string;
        email?: string;
        status?: "open" | "resolved" | "expired";
      }) => Promise<unknown>
    >(),
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
  listCommitments: api.listCommitments,
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
      sender_detail: "sender@example.com",
      subject: "Label workflow",
      snippet: "hello",
      date: "2026-05-11T10:00:00Z",
      date_label: "May 11",
      date_full: "May 11, 2026, 10:00 AM",
      date_relative: "now",
      labels: [{ id: "label-work", name: "Work", kind: "user", color: "#fb4c2f" }],
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
      labels: [{ id: "label-work", name: "Work", kind: "user", color: "#fb4c2f" }],
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
    useUiPrefs.setState({ readerLayout: "split" });
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
    api.summarizeThread.mockResolvedValue({
      kind: "ThreadSummary",
      model: "auto-model",
      text: "- Auto summary",
    });
    api.listCommitments.mockResolvedValue({ commitments: [] });
  });

  afterEach(() => {
    vi.clearAllMocks();
    useMailboxPane.setState({
      activePane: "mailbox",
      sidebarIndex: 0,
      suppressNextReaderFocus: false,
    });
    useModals.setState({ rightRail: null });
    useUiPrefs.setState({ readerLayout: "split" });
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

  test("direct-opened thread activates reader keyboard scrolling", async () => {
    useMailboxPane.setState({
      activePane: "mailbox",
      sidebarIndex: 0,
      suppressNextReaderFocus: false,
    });
    const originalScrollBy = HTMLElement.prototype.scrollBy;
    const scrollBy = vi.fn<(options?: ScrollToOptions) => void>();
    Object.defineProperty(HTMLElement.prototype, "scrollBy", {
      configurable: true,
      value: scrollBy,
    });

    try {
      renderWithQueryClient(<ThreadRoute />);

      expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();
      await waitFor(() => expect(useMailboxPane.getState().activePane).toBe("reader"));

      fireEvent.keyDown(window, { key: "j" });

      expect(scrollBy).toHaveBeenCalledWith({ top: 72, behavior: "smooth" });
    } finally {
      if (originalScrollBy) {
        Object.defineProperty(HTMLElement.prototype, "scrollBy", {
          configurable: true,
          value: originalScrollBy,
        });
      } else {
        delete (HTMLElement.prototype as { scrollBy?: unknown }).scrollBy;
      }
    }
  });

  test("does not steal focus when the mailbox previews another thread", async () => {
    useMailboxPane.setState({
      activePane: "mailbox",
      sidebarIndex: 0,
      suppressNextReaderFocus: true,
    });

    renderWithQueryClient(<ThreadRoute />);

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    await waitFor(() => {
      expect(useMailboxPane.getState().activePane).toBe("mailbox");
      expect(useMailboxPane.getState().suppressNextReaderFocus).toBe(false);
    });
  });

  test("h returns keyboard ownership to the mailbox list", async () => {
    renderWithQueryClient(<ThreadRoute />);

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    fireEvent.keyDown(window, { key: "h" });

    expect(useMailboxPane.getState().activePane).toBe("mailbox");
    expect(useMailboxPane.getState().suppressNextReaderFocus).toBe(true);
  });

  test("keeps lowercase f as forward and uses uppercase F for full reader", async () => {
    renderWithQueryClient(<ThreadRoute />);

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    fireEvent.keyDown(window, { key: "f" });

    expect(router.navigate).toHaveBeenCalledWith({
      to: "/compose/new",
      search: { reply: "msg-1", mode: "forward" },
    });
    expect(useUiPrefs.getState().readerLayout).toBe("split");

    fireEvent.keyDown(window, { key: "F", shiftKey: true });

    expect(useUiPrefs.getState().readerLayout).toBe("full");
  });

  test("linkifies URLs in plain reader bodies", async () => {
    api.fetchThread.mockResolvedValueOnce({
      ...thread,
      bodies: [
        {
          message_id: "msg-1",
          text_plain: "Read https://example.com/docs for details",
          reader_text: "Read https://example.com/docs for details",
          text_html: null,
          attachments: [],
        },
      ],
    });
    renderWithQueryClient(<ThreadRoute />);

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    const link = screen.getByRole("link", { name: "https://example.com/docs" });
    expect(link).toHaveAttribute("href", "https://example.com/docs");
    expect(link).toHaveAttribute("target", "_blank");
  });

  test("keeps secondary reader actions in the overflow menu", async () => {
    renderWithQueryClient(<ThreadRoute />);

    expect(await screen.findByRole("button", { name: /reply/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /forward/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /summary/i })).toBeVisible();
    expect(screen.queryByRole("button", { name: /archive/i })).not.toBeInTheDocument();

    fireEvent.pointerDown(screen.getByRole("button", { name: /more message actions/i }), {
      button: 0,
      ctrlKey: false,
    });

    expect(await screen.findByRole("menuitem", { name: /archive/i })).toBeVisible();
    expect(screen.getByRole("menuitem", { name: /snooze/i })).toBeVisible();
  });

  test("opens thread context in the right rail from the reader keyboard shortcut", async () => {
    renderWithQueryClient(
      <>
        <ThreadRoute />
        <RightRail />
      </>,
    );

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    fireEvent.keyDown(window, { key: "L", shiftKey: true });

    await waitFor(() => {
      expect(useModals.getState().rightRail).toMatchObject({ kind: "thread-context" });
    });
    expect(await screen.findByRole("heading", { name: "Thread context" })).toBeVisible();
    expect(screen.getByText("1 attachment")).toBeVisible();
  });

  test("renders visible reader shortcuts and colored label chips", async () => {
    renderWithQueryClient(<ThreadRoute />);

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    expect(screen.getByRole("button", { name: /reply r/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /forward f/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /summary y/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /more message actions/i })).toBeVisible();
    expect(screen.queryByRole("button", { name: /reply all a/i })).not.toBeInTheDocument();

    const label = screen.getAllByText("Work")[0];
    expect(label).toBeDefined();
    expect(label!).toHaveStyle({ color: "#fb4c2f" });
  });

  test("renders cached thread summaries inline without regenerating", async () => {
    api.fetchThread.mockResolvedValueOnce({
      ...thread,
      summary: {
        generated_at: "2026-05-11T10:02:00Z",
        model: "cached-model",
        text: "ACTION REQUIRED — approve launch checklist\n\n- Cached useful point",
      },
    });
    renderWithQueryClient(
      <>
        <ThreadRoute />
        <RightRail />
      </>,
    );

    expect(await screen.findByText("AI overview")).toBeVisible();
    expect(screen.getByText("cached-model")).toBeVisible();
    const cachedSummaryToggle = screen.getByRole("button", {
      name: /AI overview\s*cached-model/i,
    });
    if (cachedSummaryToggle.getAttribute("aria-expanded") === "false") {
      fireEvent.click(cachedSummaryToggle);
    }
    expect(screen.getByText("ACTION REQUIRED — approve launch checklist")).toBeVisible();
    expect(api.summarizeThread).not.toHaveBeenCalled();
  });

  test("renders open commitment chips for the primary sender", async () => {
    api.listCommitments.mockResolvedValueOnce({
      commitments: [
        {
          id: "commitment-1",
          direction: "theirs",
          who_owes: "Sender",
          what: "Send launch dates",
          by_when: "2026-05-20T00:00:00Z",
        },
      ],
    });

    renderWithQueryClient(<ThreadRoute />);

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();
    expect(await screen.findByLabelText("Open commitments")).toBeVisible();
    expect(screen.getByText("Send launch dates")).toBeVisible();
    expect(api.listCommitments).toHaveBeenCalledWith({
      accountId: "account-1",
      email: "sender@example.com",
      status: "open",
    });
  });

  test("auto-generates thread summaries inline above the email instead of the right rail", async () => {
    api.summarizeThread.mockResolvedValueOnce({
      kind: "ThreadSummary",
      model: "summary-model",
      text: "- First useful point\n- Second useful point",
    });
    renderWithQueryClient(
      <>
        <ThreadRoute />
        <RightRail />
      </>,
    );

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    expect(await screen.findByText("AI overview")).toBeVisible();
    expect(screen.getByText("summary-model")).toBeVisible();
    const summaryToggle = screen.getByRole("button", { name: /AI overview\s*summary-model/i });
    if (summaryToggle.getAttribute("aria-expanded") === "false") {
      fireEvent.click(summaryToggle);
    }
    expect(screen.queryByText(/"kind"/)).not.toBeInTheDocument();
  });

  test("renders sender profile as relationship stats in the right rail", async () => {
    api.fetchSenderProfile.mockResolvedValueOnce({
      kind: "SenderProfile",
      profile: {
        account_id: "account-1",
        email: "sender@example.com",
        display_name: "Sender",
        first_seen_at: "2026-05-01T09:00:00Z",
        last_seen_at: "2026-05-11T10:00:00Z",
        last_inbound_at: "2026-05-11T10:00:00Z",
        last_outbound_at: "2026-05-10T10:00:00Z",
        total_inbound: 12,
        total_outbound: 4,
        replied_count: 3,
        cadence_days_p50: 2.5,
        is_list_sender: false,
        list_id: null,
        open_thread_count: 2,
        inbound_storage_bytes: 4096,
        outbound_storage_bytes: 1024,
        attachment_count: 3,
        attachment_bytes: 2048,
      },
    });
    renderWithQueryClient(
      <>
        <ThreadRoute />
        <RightRail />
      </>,
    );

    expect(await screen.findByRole("heading", { name: "Label workflow" })).toBeVisible();

    fireEvent.keyDown(window, { key: "p" });

    expect(await screen.findByText("sender@example.com")).toBeVisible();
    expect(screen.getByText("Storage")).toBeVisible();
    expect(screen.getByText("Asymmetry")).toBeVisible();
    expect(screen.getByText("Reply rate")).toBeVisible();
    expect(screen.queryByText(/"kind"/)).not.toBeInTheDocument();
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
