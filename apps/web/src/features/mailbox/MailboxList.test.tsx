import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { gistOrder, MailboxList, type MailboxListProps } from "./MailboxList";
import type { MessageGroupView, MessageRowView, MutationResponse } from "./types";
import { usePendingMailOps, useProjectedGroups } from "@/features/mail-actions/pendingMailOps";
import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import { getRegistry, invokeAction, isAvailable, snapshotActionContext } from "@/lib/actions";
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
}));
vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  archiveMessages: api.archiveMessages,
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

/** The list as the inbox renders it: pending ops projected over server rows. */
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
  test("the list takes focus and the cursor starts on the first row", () => {
    renderList();

    expect(document.activeElement).toBe(list());
    expect(cursor()).toBe("mail-row-thread-thread-1");
  });

  test("j and k move the cursor, clamped at the ends", () => {
    renderList();

    type({ key: "j" }, { key: "j" });
    expect(cursor()).toBe("mail-row-thread-thread-3");
    type({ key: "k" });
    expect(cursor()).toBe("mail-row-thread-thread-2");
    type({ key: "k" }, { key: "k" }, { key: "k" });
    expect(cursor()).toBe("mail-row-thread-thread-1");
  });

  test("G jumps to the last row and g g back to the first", () => {
    renderList();

    type({ key: "G", shiftKey: true });
    expect(cursor()).toBe("mail-row-thread-thread-4");
    type({ key: "g" }, { key: "g" });
    expect(cursor()).toBe("mail-row-thread-thread-1");
  });

  test("x selects the row and moves down", () => {
    renderList();

    type({ key: "x" });
    expect(selectedIds()).toEqual(["thread-thread-1"]);
    expect(cursor()).toBe("mail-row-thread-thread-2");

    type({ key: "x" });
    expect(selectedIds()).toEqual(["thread-thread-1", "thread-thread-2"]);
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

  test("* u selects unread rows and * n clears", () => {
    renderList();

    type({ key: "*", shiftKey: true }, { key: "u" });
    expect(selectedIds()).toEqual(["thread-thread-2", "thread-thread-4"]);

    type({ key: "*", shiftKey: true }, { key: "n" });
    expect(selectedIds()).toEqual([]);
  });

  test("Escape leaves visual mode, then clears the selection, then closes the thread", () => {
    renderList();

    type({ key: "V", shiftKey: true }, { key: "j" });
    expect(selectedIds()).toEqual(["thread-thread-1", "thread-thread-2"]);

    type({ key: "Escape" });
    // Visual mode ends; the selection stays and j no longer extends it.
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

  test("clicking a row opens it without moving focus to the reader", () => {
    renderList();

    fireEvent.click(screen.getByRole("option", { name: /Subject 3/ }));

    expect(onOpenRow).toHaveBeenCalledWith(rows[2], { focusReader: false });
    expect(cursor()).toBe("mail-row-thread-thread-3");
  });

  test("with previewOnFocus, moving the cursor previews the next thread", () => {
    renderList({ previewOnFocus: true, activeThreadId: "thread-1" });

    type({ key: "j" });

    expect(onOpenRow).toHaveBeenCalledWith(rows[1], { focusReader: false });
  });

  test("keys are inactive while another pane has focus", () => {
    renderList();
    act(() => useMailboxPane.getState().setActivePane("reader"));

    type({ key: "j" });

    expect(cursor()).toBe("mail-row-thread-thread-1");
  });

  test("e archives the focused row; the cursor stays at the same position", async () => {
    let finish!: (value: MutationResponse) => void;
    api.archiveMessages.mockReturnValue(new Promise((resolve) => (finish = resolve)));
    renderList();

    type({ key: "j" }, { key: "e" });

    await vi.waitFor(() => expect(api.archiveMessages).toHaveBeenCalledWith(["msg-2"]));
    expect(screen.queryByRole("option", { name: /Subject 2/ })).not.toBeInTheDocument();
    expect(cursor()).toBe("mail-row-thread-thread-3");

    await act(async () => {
      finish({ ok: true, result: { requested: 1, succeeded: 1, skipped: 0, failed: 0 } });
      await Promise.resolve();
    });
  });

  test("the cursor follows the same row when rows are added above it", () => {
    const { rerenderList } = renderList();
    type({ key: "j" });

    rerenderList({ groups: [{ id: "today", label: "Today", rows: [row(0), ...rows] }] });

    expect(cursor()).toBe("mail-row-thread-thread-2");
  });

  test("removing the last row under the cursor clamps it to the new last row", () => {
    const { rerenderList } = renderList();
    type({ key: "j" }, { key: "j" }, { key: "j" });
    expect(cursor()).toBe("mail-row-thread-thread-4");

    rerenderList({ groups: [{ id: "today", label: "Today", rows: rows.slice(0, 3) }] });
    expect(cursor()).toBe("mail-row-thread-thread-3");
  });
});

describe("MailboxList palette actions", () => {
  test("Route out of this queue is offered only in a queue label and opens the route dialog", () => {
    const route = getRegistry().get("mail.route")!;
    const { unmount } = renderList();
    expect(isAvailable(route, snapshotActionContext())).toBe(false);
    unmount();

    renderList({ queueLabel: "Follow Up" });
    type({ key: "j" });
    const context = snapshotActionContext();
    expect(isAvailable(route, context)).toBe(true);
    act(() => invokeAction(route, context));

    expect(useMailDialogs.getState().dialog).toMatchObject({
      kind: "move",
      route: { fromQueueLabel: "Follow Up" },
      target: { messageIds: ["msg-2"] },
    });
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

describe("MailboxList rows", () => {
  test("rows announce thread size, attachments and unread state", () => {
    renderList({
      groups: [
        {
          id: "today",
          label: "Today",
          rows: [row(1, { message_count: 3, has_attachments: true, unread: true })],
        },
      ],
    });

    const option = screen.getByRole("option");
    expect(option).toHaveAccessibleName(/^Unread\./);
    expect(option).toHaveAccessibleName(/3 messages in conversation/);
    expect(option).toHaveAccessibleName(/Has attachments/);
  });

  test("renders the empty state when there are no rows", () => {
    renderList({ groups: [] });

    expect(screen.getByText("Empty")).toBeInTheDocument();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });
});

describe("gistOrder", () => {
  const rowItems = (ids: string[]) =>
    ids.map((id) => ({ kind: "row" as const, row: { thread_id: id } as MessageRowView }));
  test("visible rows first, then the next few below, then the overscan above", () => {
    const flat = [
      { kind: "header" as const, id: "h", group: {} as never },
      ...rowItems(["a", "b", "c", "d", "e", "f", "g"]),
    ];
    expect(gistOrder(flat, { startIndex: 3, endIndex: 4 }, 1, 2)).toEqual([
      "c",
      "d",
      "e",
      "f",
      "b",
      "a",
    ]);
  });
});
