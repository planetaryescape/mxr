import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { MailboxList, type MailboxListProps } from "./MailboxList";
import type { MessageGroupView, MessageRowView, MutationResponse } from "./types";
import { usePendingMailOps, useProjectedGroups } from "@/features/mail-actions/pendingMailOps";
import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import { getRegistry, snapshotActionContext } from "@/lib/actions";
import { installKeyDispatcher } from "@/lib/keys/dispatcher";
import { useKeyScope } from "@/state/keyScopeStore";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useSelection } from "@/state/selectionStore";

// jsdom has no layout, so the real virtualizer renders nothing. Render every
// item; scrolling isn't what these tests are about.
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: (options: { count: number; getItemKey: (index: number) => string | number }) => ({
    getVirtualItems: () =>
      Array.from({ length: options.count }, (_, index) => ({
        index,
        key: options.getItemKey(index),
        start: index * 40,
        end: index * 40 + 40,
        size: 40,
        lane: 0,
      })),
    getTotalSize: () => options.count * 40,
    measure: () => undefined,
    measureElement: () => undefined,
    scrollToIndex: () => undefined,
  }),
}));

vi.mock("./BulkActionBar", () => ({ BulkActionBar: () => null }));

const api = vi.hoisted(() => ({
  archiveMessages: vi.fn<(ids: string[]) => Promise<MutationResponse>>(),
  archiveMessagesRecoverably: vi.fn<(ids: string[]) => Promise<MutationResponse>>(),
}));
vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  archiveMessages: api.archiveMessages,
}));
vi.mock("@/features/mail-actions/archiveRecovery", () => ({
  archiveMessagesRecoverably: api.archiveMessagesRecoverably,
}));

function row(n: number, overrides: Partial<MessageRowView> = {}): MessageRowView {
  return {
    id: `msg-${n}`,
    kind: "thread",
    thread_id: `thread-${n}`,
    provider_id: `provider-${n}`,
    sender: `Sender ${n}`,
    subject: `Subject ${n}`,
    snippet: "Snippet",
    date: "2026-05-11T10:00:00Z",
    date_label: "May 11",
    date_full: "May 11, 2026, 10:00 AM",
    date_relative: "now",
    unread: false,
    starred: false,
    has_attachments: false,
    message_ids: [`msg-${n}`],
    ...overrides,
  };
}

const rows = [row(1), row(2, { unread: true }), row(3), row(4, { unread: true })];
const groups: MessageGroupView[] = [{ id: "today", label: "Today", rows }];

function Inbox(props: Omit<MailboxListProps, "scopeKey" | "empty" | "label">) {
  const projected = useProjectedGroups(props.groups, { kind: "inbox" });
  return (
    <MailboxList
      {...props}
      groups={projected}
      scopeKey="inbox"
      empty={<p>Empty</p>}
      label="Inbox"
    />
  );
}

let onOpenRow: ReturnType<typeof vi.fn<MailboxListProps["onOpenRow"]>>;
let onCloseThread: ReturnType<typeof vi.fn<() => void>>;
let uninstall: () => void;

function renderList(props: Partial<MailboxListProps> = {}) {
  const view = render(
    <Inbox groups={groups} onOpenRow={onOpenRow} onCloseThread={onCloseThread} {...props} />,
  );
  return {
    ...view,
    rerenderList: (next: Partial<MailboxListProps>) =>
      view.rerender(
        <Inbox
          groups={groups}
          onOpenRow={onOpenRow}
          onCloseThread={onCloseThread}
          {...props}
          {...next}
        />,
      ),
  };
}

function list() {
  return screen.getByRole("listbox", { name: "Inbox" });
}

function type(...keys: KeyboardEventInit[]) {
  for (const init of keys) fireEvent.keyDown(document.activeElement ?? document.body, init);
}

function cursor(): string | null {
  return list().getAttribute("aria-activedescendant");
}

function selectedIds(): string[] {
  return within(list())
    .getAllByRole("option")
    .filter((option) => option.getAttribute("aria-selected") === "true")
    .map((option) => option.id.replace("mail-row-", ""));
}

beforeEach(() => {
  api.archiveMessagesRecoverably.mockReset();
  onOpenRow = vi.fn<MailboxListProps["onOpenRow"]>();
  onCloseThread = vi.fn<() => void>();
  useMailboxPane.setState({
    activePane: "mailbox",
    sidebarIndex: 0,
    suppressNextReaderFocus: false,
  });
  useSelection.setState({ scope: null, ids: new Set(), lastClickedId: null });
  useKeyScope.setState({ stack: [], pendingPrefix: null });
  usePendingMailOps.setState({ ops: [] });
  useMailDialogs.setState({ dialog: null });
  uninstall = installKeyDispatcher(window, {
    registry: getRegistry(),
    context: snapshotActionContext,
    mac: false,
  });
});

afterEach(() => {
  uninstall();
  vi.clearAllMocks();
});

describe("MailboxList keyboard", () => {
  test("j and k move the cursor, clamped at the ends", () => {
    renderList();

    type({ key: "j" }, { key: "j" });
    expect(cursor()).toBe("mail-row-thread-thread-3");
    type({ key: "k" });
    expect(cursor()).toBe("mail-row-thread-thread-2");
    type({ key: "k" }, { key: "k" }, { key: "k" });
    expect(cursor()).toBe("mail-row-thread-thread-1");
  });

  test("V starts visual mode and j extends the selection", () => {
    renderList();

    type({ key: "j" }, { key: "V", shiftKey: true });
    expect(selectedIds()).toEqual(["thread-thread-2"]);
    type({ key: "j" }, { key: "j" });
    expect(selectedIds()).toEqual(["thread-thread-2", "thread-thread-3", "thread-thread-4"]);
    type({ key: "k" });
    expect(selectedIds()).toEqual(["thread-thread-2", "thread-thread-3"]);
  });

  test("Escape leaves visual mode, then clears the selection, then closes the thread", () => {
    renderList();

    type({ key: "V", shiftKey: true }, { key: "j" });
    expect(selectedIds()).toEqual(["thread-thread-1", "thread-thread-2"]);

    type({ key: "Escape" });
    type({ key: "j" });
    expect(selectedIds()).toEqual(["thread-thread-1", "thread-thread-2"]);

    type({ key: "Escape" });
    expect(selectedIds()).toEqual([]);
    expect(onCloseThread).not.toHaveBeenCalled();

    type({ key: "Escape" });
    expect(onCloseThread).toHaveBeenCalledTimes(1);
  });

  test("Enter opens the focused row in the reader", () => {
    renderList();

    type({ key: "j" }, { key: "Enter" });

    expect(onOpenRow).toHaveBeenCalledWith(rows[1], { focusReader: true });
  });
});

describe("MailboxList readOnly", () => {
  test("navigates but ignores selection and mutation keys", () => {
    renderList({ readOnly: true });

    type({ key: "x" }, { key: "e" });

    expect(list()).toHaveAttribute("aria-multiselectable", "false");
    expect(selectedIds()).toEqual([]);
    expect(api.archiveMessages).not.toHaveBeenCalled();
    expect(usePendingMailOps.getState().ops).toEqual([]);

    type({ key: "j" });
    expect(cursor()).toBe("mail-row-thread-thread-2");
  });
});
