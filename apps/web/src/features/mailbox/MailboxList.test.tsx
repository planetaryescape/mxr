/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { MailboxList } from "./MailboxList";
import { MailboxRow } from "./MailboxRow";
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
    useMailboxPane.setState({
      activePane: "mailbox",
      sidebarIndex: 0,
      suppressNextReaderFocus: false,
    });
    useSelection.setState({ scope: null, ids: new Set(), lastClickedId: null });
  });

  afterEach(() => {
    vi.clearAllMocks();
    useSelection.getState().clear();
    useMailboxPane.setState({
      activePane: "mailbox",
      sidebarIndex: 0,
      suppressNextReaderFocus: false,
    });
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

  test("keeps keyboard focus on the same message when rows shift", async () => {
    const { rerender } = render(<MailboxList groups={groups} mailboxPath="/m/inbox" />);

    expect(await screen.findByText(/3 loaded/i)).toBeVisible();

    fireEvent.keyDown(window, { key: "j" });

    const first = rows[0];
    if (!first) throw new Error("missing first row");
    const prepended: MessageRowView = {
      ...first,
      id: "msg-0",
      thread_id: "thread-0",
      provider_id: "provider-0",
      subject: "Subject 0",
    };
    rerender(
      <MailboxList
        groups={[{ id: "today", label: "Today", rows: [prepended, ...rows] }]}
        mailboxPath="/m/inbox"
      />,
    );

    fireEvent.keyDown(window, { key: "x" });

    expect([...useSelection.getState().ids]).toEqual(["msg-2"]);
  });

  test("jumps to the top with gg and bottom with G", async () => {
    render(<MailboxList groups={groups} mailboxPath="/m/inbox" />);

    expect(await screen.findByText(/3 loaded/i)).toBeVisible();

    fireEvent.keyDown(window, { key: "G", shiftKey: true });
    fireEvent.keyDown(window, { key: "x" });

    expect([...useSelection.getState().ids]).toEqual(["msg-3"]);

    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.keyDown(window, { key: "g" });
    fireEvent.keyDown(window, { key: "g" });
    fireEvent.keyDown(window, { key: "x" });

    expect([...useSelection.getState().ids]).toEqual(["msg-1"]);
  });

  test("keeps mailbox pane active when keyboard preview opens the next thread", async () => {
    render(
      <MailboxList
        groups={groups}
        mailboxPath="/m/inbox"
        activeThreadId="thread-1"
        previewOnFocus
      />,
    );

    expect(await screen.findByText(/3 loaded/i)).toBeVisible();

    fireEvent.keyDown(window, { key: "j" });

    expect(useMailboxPane.getState().activePane).toBe("mailbox");
    expect(useMailboxPane.getState().suppressNextReaderFocus).toBe(true);
    expect(router.navigate).toHaveBeenCalledWith({
      to: "/m/$mailbox/$threadId",
      params: { mailbox: "inbox", threadId: "thread-2" },
    });
  });

  test("escape closes an open clicked thread when there is no active selection", async () => {
    render(
      <MailboxList
        groups={groups}
        mailboxPath="/m/inbox"
        activeThreadId="thread-1"
        previewOnFocus
      />,
    );

    expect(await screen.findByText(/3 loaded/i)).toBeVisible();

    fireEvent.keyDown(window, { key: "Escape" });

    expect(router.navigate).toHaveBeenCalledWith({ to: "/m/inbox" });
  });

  test("shows attachment status in the mailbox row", async () => {
    render(
      <MailboxRow
        row={{ ...rows[0]!, has_attachments: true, attachment_filename: "quote.pdf" }}
        selected={false}
        focused={false}
        onOpen={vi.fn<() => void>()}
        onFocusPane={vi.fn<() => void>()}
        onToggleSelection={vi.fn<(shift: boolean) => void>()}
      />,
    );

    expect(screen.getByLabelText("Has attachments")).toBeVisible();
    expect(screen.getByRole("article", { name: /has attachments/i })).toBeVisible();
  });

  test("shows conversation thread count in the mailbox row", async () => {
    render(
      <MailboxRow
        row={{ ...rows[0]!, message_count: 4 }}
        selected={false}
        focused={false}
        onOpen={vi.fn<() => void>()}
        onFocusPane={vi.fn<() => void>()}
        onToggleSelection={vi.fn<(shift: boolean) => void>()}
      />,
    );

    expect(screen.getByLabelText("Conversation thread with 4 messages")).toBeVisible();
    expect(
      screen.getByRole("article", { name: /conversation thread with 4 messages/i }),
    ).toBeVisible();
  });

  test("shows open commitment count in the mailbox row", async () => {
    render(
      <MailboxRow
        row={{ ...rows[0]!, open_commitment_count: 2 }}
        selected={false}
        focused={false}
        onOpen={vi.fn<() => void>()}
        onFocusPane={vi.fn<() => void>()}
        onToggleSelection={vi.fn<(shift: boolean) => void>()}
      />,
    );

    expect(screen.getByLabelText("2 open commitments")).toBeVisible();
    expect(screen.getByRole("article", { name: /2 open commitments/i })).toBeVisible();
  });
});
