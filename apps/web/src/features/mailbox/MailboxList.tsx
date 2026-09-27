import { useVirtualizer } from "@tanstack/react-virtual";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { toast } from "sonner";

import { BulkActionBar } from "./BulkActionBar";
import { MailboxRow, RowActionChip, type RowAction, type RowQuickAction } from "./MailboxRow";
import { rowKey } from "./rowKey";
import type { MessageGroupView, MessageRowView } from "./types";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { performMailAction } from "@/features/mail-actions/mailMutations";
import { createMailVerbs } from "@/features/mail-actions/mailVerbs";
import { rowMessageIds } from "@/features/mail-actions/pendingMailOps";
import { targetFromRows, type MailTarget } from "@/features/mail-actions/target";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { useScopeController } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useSelection } from "@/state/selectionStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

export interface MailboxListProps {
  groups: MessageGroupView[];
  /** Selection is cleared when this changes (lens, query, account). */
  scopeKey: string;
  activeThreadId?: string;
  /** While a thread is open, moving the cursor opens the next one (TUI three-pane). */
  previewOnFocus?: boolean;
  onOpenRow: (row: MessageRowView, options: { focusReader: boolean }) => void;
  onCloseThread?: () => void;
  hasMore?: boolean;
  /** Open the list's quick filter (TUI Ctrl-f). */
  onFilter?: () => void;
  loadingMore?: boolean;
  onLoadMore?: () => void;
  /** Lists of aggregates that aren't mutable messages: navigation only. */
  readOnly?: boolean;
  /** The list's own verb for a row (wake, done): `w`, or a click on the row's chip. */
  rowAction?: RowAction;
  /** Current queue label, enabling "Route out of this queue". */
  queueLabel?: string;
  empty: ReactNode;
  label: string;
}

type FlatItem =
  | { kind: "header"; id: string; label: string }
  | { kind: "row"; row: MessageRowView };

function flatten(groups: MessageGroupView[]): FlatItem[] {
  const items: FlatItem[] = [];
  for (const group of groups) {
    items.push({ kind: "header", id: `header-${group.id}`, label: group.label });
    for (const row of group.rows) items.push({ kind: "row", row });
  }
  return items;
}

function domId(row: MessageRowView): string {
  return `mail-row-${rowKey(row)}`;
}

const ROW_ESTIMATE = { compact: 38, regular: 64, comfortable: 80 } as const;

export function MailboxList({
  groups,
  scopeKey,
  activeThreadId,
  previewOnFocus = false,
  onOpenRow,
  onCloseThread,
  hasMore = false,
  onFilter,
  loadingMore = false,
  onLoadMore,
  readOnly = false,
  rowAction,
  queueLabel,
  empty,
  label,
}: MailboxListProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const density = useUiPrefs((s) => s.density);
  const setListMode = useUiPrefs((s) => s.setListMode);
  const listMode = useUiPrefs((s) => s.listMode);
  const activePane = useMailboxPane((s) => s.activePane);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const selectedIds = useSelection((s) => s.ids);
  const setSelectionScope = useSelection((s) => s.setScope);
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const lastIndexRef = useRef(0);
  const [visualAnchor, setVisualAnchor] = useState<string | null>(null);

  const flat = useMemo(() => flatten(groups), [groups]);
  const rows = useMemo(
    () => flat.flatMap((item) => (item.kind === "row" ? [item.row] : [])),
    [flat],
  );
  const rowIndexById = useMemo(
    () => new Map(rows.map((row, index) => [rowKey(row), index])),
    [rows],
  );
  const flatIndexById = useMemo(() => {
    const map = new Map<string, number>();
    flat.forEach((item, index) => {
      if (item.kind === "row") map.set(rowKey(item.row), index);
    });
    return map;
  }, [flat]);

  useEffect(() => setSelectionScope(scopeKey), [scopeKey, setSelectionScope]);
  useEffect(() => setVisualAnchor(null), [scopeKey]);

  // Resolve the cursor. When the focused row disappears (archived,
  // filtered) the cursor stays at the same position, so the next
  // conversation comes up under it, the way the TUI behaves.
  const focusedIndex = (() => {
    if (rows.length === 0) return -1;
    const index = focusedId ? rowIndexById.get(focusedId) : undefined;
    if (index !== undefined) return index;
    return Math.min(lastIndexRef.current, rows.length - 1);
  })();
  const focusedRow = focusedIndex >= 0 ? rows[focusedIndex] : undefined;
  useLayoutEffect(() => {
    if (focusedIndex >= 0) lastIndexRef.current = focusedIndex;
    if (focusedRow && rowKey(focusedRow) !== focusedId) setFocusedId(rowKey(focusedRow));
  }, [focusedId, focusedIndex, focusedRow]);

  // Follow the reader: opening a thread elsewhere puts the cursor on it.
  // Only when the cursor is on another conversation: in message mode a
  // thread has several rows, and snapping back to its first row would trap
  // j on the second.
  const focusedThreadId = focusedRow?.thread_id;
  useEffect(() => {
    if (!activeThreadId || focusedThreadId === activeThreadId) return;
    const row = rows.find((item) => item.thread_id === activeThreadId);
    if (row) setFocusedId(rowKey(row));
  }, [activeThreadId, focusedThreadId, rows]);

  const virtualizer = useVirtualizer({
    count: flat.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) => (flat[index]?.kind === "header" ? 30 : ROW_ESTIMATE[density]),
    overscan: 12,
    getItemKey: (index) => {
      const item = flat[index];
      return item ? (item.kind === "header" ? item.id : rowKey(item.row)) : index;
    },
  });
  const virtualItems = virtualizer.getVirtualItems();

  useEffect(() => {
    virtualizer.measure();
  }, [density, virtualizer]);

  useEffect(() => {
    const last = virtualItems.at(-1);
    if (!last || !hasMore || loadingMore || !onLoadMore) return;
    if (last.index >= flat.length - 10) onLoadMore();
  }, [flat.length, hasMore, loadingMore, onLoadMore, virtualItems]);

  // Opening or closing the reader changes the list's width, rows reflow
  // and re-measure, and the virtualizer's offsets shift. Keep the cursor
  // row where it was on screen so returning from the reader lands the user
  // exactly where they left.
  const focusedDomIdRef = useRef<string | null>(null);
  focusedDomIdRef.current = focusedRow ? domId(focusedRow) : null;
  const anchorRef = useRef<{ id: string; top: number } | null>(null);
  const captureAnchor = useCallback(() => {
    const list = scrollRef.current;
    const id = focusedDomIdRef.current;
    const row = id ? document.getElementById(id) : null;
    anchorRef.current =
      list && id && row
        ? { id, top: row.getBoundingClientRect().top - list.getBoundingClientRect().top }
        : null;
  }, []);
  useEffect(() => {
    requestAnimationFrame(captureAnchor);
  }, [captureAnchor, focusedId]);
  useEffect(() => {
    const list = scrollRef.current;
    if (!list || typeof ResizeObserver === "undefined") return;
    let width = list.clientWidth;
    let settling = 0;
    const observer = new ResizeObserver(() => {
      if (list.clientWidth === width) return;
      width = list.clientWidth;
      const anchor = anchorRef.current;
      if (!anchor) return;
      cancelAnimationFrame(settling);
      // Rows re-measure over the next frames; put the row back each time.
      let frames = 0;
      const settle = () => {
        const row = document.getElementById(anchor.id);
        if (row) {
          const top = row.getBoundingClientRect().top - list.getBoundingClientRect().top;
          list.scrollTop += top - anchor.top;
        }
        frames += 1;
        if (frames < 6) {
          settling = requestAnimationFrame(settle);
        } else {
          settling = 0;
          anchorRef.current = anchor;
        }
      };
      settling = requestAnimationFrame(settle);
    });
    const onScroll = () => {
      if (!settling) captureAnchor();
    };
    observer.observe(list);
    list.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      cancelAnimationFrame(settling);
      observer.disconnect();
      list.removeEventListener("scroll", onScroll);
    };
  }, [captureAnchor]);

  const scrollToRow = useCallback(
    (row: MessageRowView | undefined, align: "auto" | "start" | "center" | "end" = "auto") => {
      const index = row ? flatIndexById.get(rowKey(row)) : undefined;
      if (index !== undefined) virtualizer.scrollToIndex(index, { align });
    },
    [flatIndexById, virtualizer],
  );

  const listFocused = activePane === "mailbox";
  useShortcutScope("list", listFocused);

  // DOM focus follows the logical pane so screen readers and Tab agree.
  useEffect(() => {
    if (!listFocused) return;
    const active = document.activeElement;
    if (
      active &&
      active !== document.body &&
      active.closest("input, textarea, [contenteditable=true], [role=dialog]")
    ) {
      return;
    }
    scrollRef.current?.focus({ preventScroll: true });
  }, [listFocused]);

  const moveTo = useCallback(
    (index: number, align: "auto" | "start" | "center" | "end" = "auto") => {
      if (rows.length === 0) return;
      const next = rows[Math.max(0, Math.min(rows.length - 1, index))];
      if (!next) return;
      setFocusedId(rowKey(next));
      scrollToRow(next, align);
      if (visualAnchor) {
        // If the anchor row left the list, restart the range at the cursor
        // rather than selecting everything from the top.
        const a = rowIndexById.get(visualAnchor) ?? rowIndexById.get(rowKey(next)) ?? 0;
        const b = rowIndexById.get(rowKey(next)) ?? 0;
        const [start, end] = a < b ? [a, b] : [b, a];
        useSelection.getState().selectMany(rows.slice(start, end + 1).map(rowKey));
      }
      if (previewOnFocus && next.thread_id !== activeThreadId) {
        onOpenRow(next, { focusReader: false });
      }
    },
    [activeThreadId, onOpenRow, previewOnFocus, rowIndexById, rows, scrollToRow, visualAnchor],
  );

  const pageSize = () => {
    const height = scrollRef.current?.clientHeight ?? 600;
    return Math.max(1, Math.floor(height / 2 / ROW_ESTIMATE[density]));
  };

  const viewportIndex = (where: "top" | "middle") => {
    const scrollTop = scrollRef.current?.scrollTop ?? 0;
    const height = scrollRef.current?.clientHeight ?? 0;
    const target = where === "top" ? scrollTop + 4 : scrollTop + height / 2;
    const item = virtualItems.find((virtual) => virtual.start + virtual.size >= target);
    for (let index = item?.index ?? 0; index < flat.length; index += 1) {
      const candidate = flat[index];
      if (candidate?.kind === "row") return rowIndexById.get(rowKey(candidate.row)) ?? 0;
    }
    return 0;
  };

  const selectedRows = () => {
    const ids = useSelection.getState().ids;
    return rows.filter((row) => ids.has(rowKey(row)));
  };

  const getTarget = (): MailTarget | null => {
    if (readOnly) return null;
    const selected = selectedRows();
    if (selected.length > 0) return targetFromRows(selected, "list");
    return focusedRow ? targetFromRows([focusedRow], "list") : null;
  };

  const selectWhere = (predicate: (row: MessageRowView) => boolean) => {
    useSelection.getState().selectMany(rows.filter(predicate).map(rowKey));
  };

  const verbs = createMailVerbs({
    getTarget,
    composeSurface: "overlay",
    afterLeave: () => setVisualAnchor(null),
  });

  useScopeController("list", {
    ...(readOnly ? {} : verbs),
    down: () => moveTo(focusedIndex + 1),
    up: () => moveTo(focusedIndex - 1),
    top: () => moveTo(0, "start"),
    bottom: () => moveTo(rows.length - 1, "end"),
    pageDown: () => moveTo(focusedIndex + pageSize()),
    pageUp: () => moveTo(focusedIndex - pageSize()),
    viewportTop: () => moveTo(viewportIndex("top")),
    viewportMiddle: () => moveTo(viewportIndex("middle")),
    open: () => focusedRow && onOpenRow(focusedRow, { focusReader: true }),
    ...(rowAction ? { rowAction: () => focusedRow && rowAction.run(focusedRow) } : {}),
    focusSidebar: () => setActivePane("sidebar"),
    ...(onFilter ? { filter: onFilter } : {}),
    escape: () => {
      if (visualAnchor) {
        setVisualAnchor(null);
        return;
      }
      if (useSelection.getState().ids.size > 0) {
        useSelection.getState().clear();
        return;
      }
      onCloseThread?.();
    },
    ...(readOnly
      ? {}
      : {
          toggleSelect: () => {
            if (!focusedRow) return;
            useSelection.getState().toggle(rowKey(focusedRow));
            moveTo(focusedIndex + 1);
          },
          visual: () => {
            if (!focusedRow) return;
            if (visualAnchor) {
              setVisualAnchor(null);
              return;
            }
            setVisualAnchor(rowKey(focusedRow));
            useSelection
              .getState()
              .selectMany([...useSelection.getState().ids, rowKey(focusedRow)]);
            toast.info("Visual mode: j/k extend, Esc to leave", {
              id: "visual-mode",
              duration: 2500,
            });
          },
          selectAll: () => selectWhere(() => true),
          selectNone: () => useSelection.getState().clear(),
          selectRead: () => selectWhere((row) => !row.unread),
          selectUnread: () => selectWhere((row) => row.unread),
          selectStarred: () => selectWhere((row) => row.starred),
          toggleThreads: () => setListMode(listMode === "threads" ? "messages" : "threads"),
          ...(queueLabel
            ? {
                route: () => {
                  const target = getTarget();
                  if (!target) return;
                  openMailDialog({ kind: "move", target, route: { fromQueueLabel: queueLabel } });
                },
              }
            : {}),
        }),
  });

  const onQuickAction = useCallback((row: MessageRowView, action: RowQuickAction) => {
    const ids = rowMessageIds(row);
    switch (action) {
      case "archive":
        void performMailAction("archive", ids);
        break;
      case "trash":
        void performMailAction("trash", ids);
        break;
      case "toggleRead":
        void performMailAction(row.unread ? "read" : "unread", ids);
        break;
      case "toggleStar":
        void performMailAction(row.starred ? "unstar" : "star", ids);
        break;
      case "snooze":
        openMailDialog({ kind: "snooze", target: targetFromRows([row], "list") });
        break;
    }
  }, []);

  const onToggleSelection = useCallback(
    (row: MessageRowView, shift: boolean) => {
      const selection = useSelection.getState();
      if (shift && selection.lastClickedId) {
        const a = rowIndexById.get(selection.lastClickedId);
        const b = rowIndexById.get(rowKey(row));
        if (a !== undefined && b !== undefined) {
          const [start, end] = a < b ? [a, b] : [b, a];
          selection.selectRange(rows.slice(start, end + 1).map(rowKey));
          return;
        }
      }
      selection.toggle(rowKey(row));
    },
    [rowIndexById, rows],
  );

  const handleOpen = useCallback(
    (row: MessageRowView) => {
      setFocusedId(rowKey(row));
      setActivePane("mailbox");
      onOpenRow(row, { focusReader: false });
    },
    [onOpenRow, setActivePane],
  );

  if (rows.length === 0) return <>{empty}</>;

  const selecting = selectedIds.size > 0;

  return (
    <div className="@container relative flex min-h-0 flex-1 flex-col">
      <div
        ref={scrollRef}
        role="listbox"
        aria-label={label}
        aria-multiselectable={!readOnly}
        aria-activedescendant={focusedRow ? domId(focusedRow) : undefined}
        tabIndex={0}
        className="min-h-0 flex-1 overflow-y-auto outline-none"
        data-active-pane={listFocused ? "true" : undefined}
        data-testid="mailbox-list"
        onFocus={() => setActivePane("mailbox")}
        onMouseDown={() => setActivePane("mailbox")}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualItems.map((virtualItem) => {
            const item = flat[virtualItem.index];
            if (!item) return null;
            return (
              <div
                key={virtualItem.key}
                data-index={virtualItem.index}
                ref={virtualizer.measureElement}
                className="absolute left-0 top-0 w-full"
                style={{ transform: `translateY(${virtualItem.start}px)` }}
              >
                {item.kind === "header" ? (
                  <div
                    role="presentation"
                    className="flex h-[30px] items-end border-b border-border/60 bg-background px-4 pb-1 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground"
                  >
                    {item.label}
                  </div>
                ) : (
                  <MailboxRow
                    row={item.row}
                    domId={domId(item.row)}
                    selected={!readOnly && selectedIds.has(rowKey(item.row))}
                    focused={
                      listFocused &&
                      focusedRow !== undefined &&
                      rowKey(focusedRow) === rowKey(item.row)
                    }
                    open={item.row.thread_id === activeThreadId}
                    selecting={selecting}
                    readOnly={readOnly}
                    onOpen={handleOpen}
                    onToggleSelection={onToggleSelection}
                    onQuickAction={onQuickAction}
                    trailingAction={
                      rowAction ? <RowActionChip action={rowAction} row={item.row} /> : undefined
                    }
                  />
                )}
              </div>
            );
          })}
        </div>
        {hasMore || loadingMore ? (
          <div className="px-4 py-3 text-center font-mono text-2xs text-muted-foreground">
            {loadingMore ? "Loading more…" : ""}
          </div>
        ) : rows.length > 20 ? (
          <div className="px-4 py-4 text-center font-mono text-2xs text-muted-foreground">
            End of list
          </div>
        ) : null}
      </div>
      {readOnly ? null : <BulkActionBar rows={rows} getTarget={getTarget} />}
    </div>
  );
}
