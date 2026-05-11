/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { MailboxList } from "./MailboxList";
import type { MessageGroupView, MessageRowView } from "./types";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useSelection } from "@/state/selectionStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
}));

const mutation = vi.hoisted(() => ({
  mutate: vi.fn<(ids: string[]) => void>(),
  isPending: false,
}));

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => router.navigate,
}));

vi.mock("./BulkActionBar", () => ({
  BulkActionBar: () => null,
}));

vi.mock("./useOptimisticMailMutation", () => ({
  useOptimisticMailMutation: () => mutation,
}));

const rows: MessageRowView[] = ["msg-1", "msg-2", "msg-3"].map((id, index) => ({
  id,
  kind: "thread",
  thread_id: `thread-${index + 1}`,
  provider_id: `provider-${index + 1}`,
  sender: `Sender ${index + 1}`,
  subject: `Subject ${index + 1}`,
  snippet: "Snippet",
  date: "2026-05-11T10:00:00Z",
  date_label: "May 11",
  date_full: "May 11, 2026, 10:00 AM",
  date_relative: "now",
  unread: false,
  starred: false,
  has_attachments: false,
}));

const groups: MessageGroupView[] = [{ id: "today", label: "Today", rows }];

describe("MailboxList keyboard selection", () => {
  beforeEach(() => {
    useMailboxPane.setState({ activePane: "mailbox", sidebarIndex: 0 });
    useSelection.setState({ scope: null, ids: new Set(), lastClickedId: null });
  });

  afterEach(() => {
    vi.clearAllMocks();
    useSelection.getState().clear();
    useMailboxPane.setState({ activePane: "mailbox", sidebarIndex: 0 });
  });

  test("selects all visible rows with ctrl-a and clears with escape", async () => {
    render(<MailboxList groups={groups} mailboxPath="/m/inbox" />);

    expect(await screen.findByText(/3 loaded/i)).toBeVisible();

    fireEvent.keyDown(window, { key: "a", ctrlKey: true });

    expect([...useSelection.getState().ids]).toEqual(["msg-1", "msg-2", "msg-3"]);

    fireEvent.keyDown(window, { key: "Escape" });

    expect(useSelection.getState().ids.size).toBe(0);
  });

  test("extends selection from the last selected row with shift-x", async () => {
    render(<MailboxList groups={groups} mailboxPath="/m/inbox" />);

    expect(await screen.findByText(/3 loaded/i)).toBeVisible();

    fireEvent.keyDown(window, { key: "x" });
    fireEvent.keyDown(window, { key: "j" });
    fireEvent.keyDown(window, { key: "X", shiftKey: true });

    expect([...useSelection.getState().ids]).toEqual(["msg-1", "msg-2"]);
  });
});
