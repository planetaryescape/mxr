/* @vitest-environment jsdom */

import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { mailboxActions } from "./actions";
import { setActiveQueryClient } from "@/lib/queryClient";
import { useModals } from "@/state/modalStore";
import { useSelection } from "@/state/selectionStore";

const api = vi.hoisted(() => ({
  readAndArchiveMessages: vi.fn<(ids: string[]) => Promise<unknown>>(),
  unsubscribeFromSender: vi.fn<(input: { messageId: string; archive: boolean }) => Promise<unknown>>(),
}));

const toast = vi.hoisted(() => ({
  error: vi.fn<(message: string, options?: unknown) => void>(),
  success: vi.fn<(message: string, options?: unknown) => void>(),
}));

vi.mock("@/features/mailbox/api", () => ({
  readAndArchiveMessages: api.readAndArchiveMessages,
  unsubscribeFromSender: api.unsubscribeFromSender,
}));

vi.mock("sonner", () => ({
  toast,
}));

const threadData = {
  thread: {
    account_id: "account-1",
    id: "thread-1",
    latest_date: "2026-05-14T12:00:00Z",
    message_count: 2,
    participants: [],
    snippet: "hello",
    subject: "Hello",
    unread_count: 2,
  },
  messages: [
    {
      id: "msg-1",
      kind: "message",
      thread_id: "thread-1",
      provider_id: "provider-1",
      sender: "Alice",
      subject: "Hello",
      snippet: "one",
      date: "2026-05-14T12:00:00Z",
      date_label: "Today",
      date_full: "May 14, 2026",
      date_relative: "now",
      unread: true,
      starred: false,
      has_attachments: false,
    },
    {
      id: "msg-2",
      kind: "message",
      thread_id: "thread-1",
      provider_id: "provider-2",
      sender: "Alice",
      subject: "Re: Hello",
      snippet: "two",
      date: "2026-05-14T12:01:00Z",
      date_label: "Today",
      date_full: "May 14, 2026",
      date_relative: "now",
      unread: true,
      starred: false,
      has_attachments: false,
    },
  ],
  bodies: [],
};

describe("mailbox palette actions", () => {
  let client: QueryClient;

  beforeEach(() => {
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    setActiveQueryClient(client);
    window.history.pushState({}, "", "/m/inbox/thread-1");
    useSelection.setState({ ids: new Set(), lastClickedId: null, scope: null });
    useModals.setState({ commandPaletteOpen: true, rightRail: null });
    api.readAndArchiveMessages.mockResolvedValue({ ok: true });
    api.unsubscribeFromSender.mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    vi.clearAllMocks();
    client.clear();
    useModals.setState({ commandPaletteOpen: false, rightRail: null });
  });

  test("focused thread label action opens the picker with message ids from cached thread data", () => {
    client.setQueryData(["thread", "thread-1"], threadData);

    mailboxActions.find((action) => action.id === "mail.label")?.run({
      path: "/m/inbox/thread-1",
      activePane: "reader",
      selectionCount: 0,
      accountCount: 1,
      hasFocusedThread: true,
      hasFocusedMessage: false,
      isFirstAccountOnly: true,
    });

    expect(useModals.getState().rightRail).toEqual({
      kind: "label-picker",
      payload: { mode: "label-add", messageIds: ["msg-1", "msg-2"] },
    });
  });

  test("read-and-archive uses selected ids before focused thread cache", () => {
    client.setQueryData(["thread", "thread-1"], threadData);
    useSelection.setState({
      ids: new Set(["selected-1", "selected-2"]),
      lastClickedId: "selected-2",
      scope: "/m/inbox",
    });

    mailboxActions.find((action) => action.id === "mail.read-and-archive")?.run({
      path: "/m/inbox/thread-1",
      activePane: "reader",
      selectionCount: 2,
      accountCount: 1,
      hasFocusedThread: true,
      hasFocusedMessage: false,
      isFirstAccountOnly: true,
    });

    expect(api.readAndArchiveMessages).toHaveBeenCalledWith(["selected-1", "selected-2"]);
  });
});
