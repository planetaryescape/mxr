import { useVirtualizer } from "@tanstack/react-virtual";
import { useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { BulkActionBar } from "./BulkActionBar";
import { MailboxRow } from "./MailboxRow";
import type { MessageGroupView, MessageRowView } from "./types";
import { useOptimisticMailMutation } from "./useOptimisticMailMutation";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useSelection } from "@/state/selectionStore";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { Inbox } from "lucide-react";

interface MailboxListProps {
  groups: MessageGroupView[];
  mailboxPath: string;
  activeThreadId?: string;
  previewOnFocus?: boolean;
  hasMore?: boolean;
  loadingMore?: boolean;
  onLoadMore?: () => void;
  /**
   * Read-only lists drop selection, bulk actions, and message mutations
   * (star/archive/read/etc.) — keeping navigation and open. Use for
   * lists whose rows aren't directly mutable messages (e.g. stale
   * thread aggregates with no message id).
   */
  readOnly?: boolean;
  /** Optional per-row trailing control, e.g. a list-specific action. */
  rowAction?: (row: MessageRowView) => ReactNode;
}

interface FlatHeader {
  kind: "header";
  id: string;
  label: string;
}
interface FlatRow {
  kind: "row";
  row: MessageRowView;
}
type FlatItem = FlatHeader | FlatRow;

export function MailboxList({
  groups,
  mailboxPath,
  activeThreadId,
  previewOnFocus,
  hasMore = false,
  loadingMore = false,
  onLoadMore,
  readOnly = false,
  rowAction,
}: MailboxListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const pendingGoTimerRef = useRef<number | null>(null);
  const navigate = useNavigate();
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const activePane = useMailboxPane((state) => state.activePane);
  const setActivePane = useMailboxPane((state) => state.setActivePane);
  const setSuppressNextReaderFocus = useMailboxPane((state) => state.setSuppressNextReaderFocus);
  const setScope = useSelection((state) => state.setScope);
  const selectedIds = useSelection((state) => state.ids);
  const toggle = useSelection((state) => state.toggle);
  const selectRange = useSelection((state) => state.selectRange);
  const selectMany = useSelection((state) => state.selectMany);
  const clearSelection = useSelection((state) => state.clear);
  const lastClickedId = useSelection((state) => state.lastClickedId);
  const archive = useOptimisticMailMutation("archive");
  const spam = useOptimisticMailMutation("spam");
  const trash = useOptimisticMailMutation("trash");
  const star = useOptimisticMailMutation("star");
  const unstar = useOptimisticMailMutation("unstar");
  const read = useOptimisticMailMutation("read");
  const unread = useOptimisticMailMutation("unread");
  const density = useUiPrefs((state) => state.density);

  const flat = useMemo(() => flatten(groups), [groups]);
  const rows = useMemo(
    () => flat.flatMap((item) => (item.kind === "row" ? [item.row] : [])),
    [flat],
  );
  const focusedIndex = useMemo(() => {
    if (!focusedId) return rows.length > 0 ? 0 : -1;
    const index = rows.findIndex((row) => row.id === focusedId);
    return index >= 0 ? index : rows.length > 0 ? 0 : -1;
  }, [focusedId, rows]);
  const focusedRow = focusedIndex >= 0 ? rows[focusedIndex] : undefined;

  useEffect(() => setScope(mailboxPath), [mailboxPath, setScope]);

  useEffect(() => {
    setFocusedId((current) => {
      if (rows.length === 0) return null;
      if (current && rows.some((row) => row.id === current)) return current;
      return rows[0]?.id ?? null;
    });
  }, [rows]);

  useEffect(() => {
    if (!activeThreadId) return;
    const row = rows.find((item) => item.thread_id === activeThreadId);
    if (row) setFocusedId(row.id);
  }, [activeThreadId, rows]);

  const virtualizer = useVirtualizer({
    count: flat.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => {
      if (flat[index]?.kind === "header") return density === "compact" ? 26 : 32;
      if (density === "compact") return 32;
      if (density === "comfortable") return 68;
      return 52;
    },
    overscan: 10,
  });
  const virtualItems = virtualizer.getVirtualItems();

  useEffect(() => {
    virtualizer.measure();
  }, [density, virtualizer]);

  useEffect(() => {
    const lastItem = virtualItems.at(-1);
    if (!lastItem || !hasMore || loadingMore || !onLoadMore) return;
    if (lastItem.index >= flat.length - 8) onLoadMore();
  }, [flat.length, hasMore, loadingMore, onLoadMore, virtualItems]);

  useEffect(() => {
    if (!focusedRow) return;
    const flatIndex = flat.findIndex(
      (item) => item.kind === "row" && item.row.id === focusedRow.id,
    );
    if (flatIndex >= 0) virtualizer.scrollToIndex(flatIndex, { align: "auto" });
  }, [flat, focusedRow, virtualizer]);

  const openRow = useCallback(
    (row: MessageRowView, pane: "mailbox" | "reader") => {
      setActivePane(pane);
      setSuppressNextReaderFocus(pane === "mailbox");
      void navigate({
        to: "/m/$mailbox/$threadId",
        params: {
          mailbox: mailboxSegment(mailboxPath),
          threadId: row.thread_id,
        },
      });
    },
    [mailboxPath, navigate, setActivePane, setSuppressNextReaderFocus],
  );

  const clearGoPrefix = useCallback(() => {
    if (pendingGoTimerRef.current === null) return;
    window.clearTimeout(pendingGoTimerRef.current);
    pendingGoTimerRef.current = null;
  }, []);

  const focusRowAt = useCallback(
    (index: number, align: "auto" | "start" | "end" = "auto") => {
      if (rows.length === 0) return;
      const next = Math.max(0, Math.min(rows.length - 1, index));
      const row = rows[next];
      if (!row) return;
      setFocusedId(row.id);
      const flatIndex = flat.findIndex((item) => item.kind === "row" && item.row.id === row.id);
      if (flatIndex >= 0) virtualizer.scrollToIndex(flatIndex, { align });
      if (previewOnFocus && row.thread_id !== activeThreadId) openRow(row, "mailbox");
    },
    [activeThreadId, flat, openRow, previewOnFocus, rows, virtualizer],
  );

  const moveFocus = useCallback(
    (delta: number) => {
      if (rows.length === 0) return;
      const current = focusedIndex >= 0 ? focusedIndex : 0;
      focusRowAt(current + delta);
    },
    [focusRowAt, focusedIndex, rows.length],
  );

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (activePane !== "mailbox") return;
      const target = event.target;
      if (target instanceof HTMLElement) {
        if (target.closest("input, textarea, select, [contenteditable=true]")) return;
      }
      // Read-only lists support navigation + open only; block selection
      // and message mutations.
      if (readOnly) {
        const k = event.key;
        const blocked =
          ((event.metaKey || event.ctrlKey) && k.toLowerCase() === "a") ||
          ["x", "e", "s", "m"].includes(k.toLowerCase()) ||
          k === "!" ||
          k === "Delete" ||
          k === "Backspace";
        if (blocked) return;
      }
      const rowItems = rows;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
        event.preventDefault();
        selectMany(rowItems.map((row) => row.id));
      } else if (event.key === "G") {
        event.preventDefault();
        clearGoPrefix();
        focusRowAt(rowItems.length - 1, "end");
      } else if (event.key === "g") {
        if (pendingGoTimerRef.current !== null) {
          event.preventDefault();
          clearGoPrefix();
          focusRowAt(0, "start");
          return;
        }
        pendingGoTimerRef.current = window.setTimeout(() => {
          pendingGoTimerRef.current = null;
        }, 800);
      } else if (event.key === "j") {
        clearGoPrefix();
        event.preventDefault();
        moveFocus(1);
      } else if (event.key === "k") {
        clearGoPrefix();
        event.preventDefault();
        moveFocus(-1);
      } else if (event.key === "h" || event.key === "ArrowLeft") {
        clearGoPrefix();
        event.preventDefault();
        setActivePane("sidebar");
      } else if (event.key.toLowerCase() === "x") {
        clearGoPrefix();
        event.preventDefault();
        const row = rowItems[focusedIndex];
        if (!row) return;
        if (event.shiftKey && lastClickedId) {
          const ordered = rowItems.map((item) => item.id);
          const a = ordered.indexOf(lastClickedId);
          const b = ordered.indexOf(row.id);
          if (a >= 0 && b >= 0) {
            const [start, end] = a < b ? [a, b] : [b, a];
            selectRange(ordered.slice(start, end + 1));
            return;
          }
        }
        toggle(row.id);
      } else if (event.key === "Enter" || event.key === "o") {
        clearGoPrefix();
        event.preventDefault();
        const row = rowItems[focusedIndex];
        if (row) openRow(row, "reader");
      } else if (event.key === "l" || event.key === "ArrowRight") {
        clearGoPrefix();
        event.preventDefault();
        const row = rowItems[focusedIndex];
        if (row) openRow(row, "reader");
      } else if (event.key === "e") {
        clearGoPrefix();
        event.preventDefault();
        const ids =
          selectedIds.size > 0
            ? [...selectedIds]
            : rowItems[focusedIndex]
              ? [rowItems[focusedIndex].id]
              : [];
        if (ids.length > 0) archive.mutate(ids);
      } else if (event.key === "!") {
        clearGoPrefix();
        event.preventDefault();
        const ids =
          selectedIds.size > 0
            ? [...selectedIds]
            : rowItems[focusedIndex]
              ? [rowItems[focusedIndex].id]
              : [];
        if (ids.length > 0) spam.mutate(ids);
      } else if (event.key === "Delete" || event.key === "Backspace") {
        clearGoPrefix();
        event.preventDefault();
        const ids =
          selectedIds.size > 0
            ? [...selectedIds]
            : rowItems[focusedIndex]
              ? [rowItems[focusedIndex].id]
              : [];
        if (ids.length > 0) trash.mutate(ids);
      } else if (event.key === "s") {
        clearGoPrefix();
        event.preventDefault();
        const row = rowItems[focusedIndex];
        if (row) (row.starred ? unstar : star).mutate([row.id]);
      } else if (event.key === "m") {
        clearGoPrefix();
        event.preventDefault();
        const row = rowItems[focusedIndex];
        if (row) (row.unread ? read : unread).mutate([row.id]);
      } else if (event.key === "Escape") {
        clearGoPrefix();
        event.preventDefault();
        if (selectedIds.size > 0) {
          clearSelection();
        } else if (activeThreadId) {
          void navigate({ to: mailboxPath });
        }
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      clearGoPrefix();
    };
  }, [
    activePane,
    activeThreadId,
    archive,
    clearGoPrefix,
    clearSelection,
    focusRowAt,
    focusedIndex,
    lastClickedId,
    mailboxPath,
    moveFocus,
    navigate,
    openRow,
    previewOnFocus,
    read,
    readOnly,
    rows,
    selectMany,
    selectRange,
    selectedIds,
    setActivePane,
    spam,
    star,
    toggle,
    trash,
    unread,
    unstar,
  ]);

  if (rows.length === 0) {
    return (
      <EmptyState
        icon={Inbox}
        title="No mail here"
        description="This lens is empty, or sync has not delivered messages yet."
      />
    );
  }

  function toggleRow(row: MessageRowView, shift: boolean) {
    if (shift && lastClickedId) {
      const ordered = rows.map((item) => item.id);
      const a = ordered.indexOf(lastClickedId);
      const b = ordered.indexOf(row.id);
      if (a >= 0 && b >= 0) {
        const [start, end] = a < b ? [a, b] : [b, a];
        selectRange(ordered.slice(start, end + 1));
        return;
      }
    }
    toggle(row.id);
  }

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 items-center justify-between gap-3 border-b border-border px-3">
        <div className="min-w-0 truncate font-mono text-xs text-muted-foreground">
          {rows.length} loaded
          {loadingMore ? " · loading more" : hasMore ? " · scroll for more" : ""}
          {!readOnly && selectedIds.size > 0 ? ` · ${selectedIds.size} selected` : ""}
        </div>
        {readOnly ? null : (
          <div className="flex items-center gap-1">
            <Button
              variant="outline"
              size="xs"
              onClick={() => selectMany(rows.map((row) => row.id))}
            >
              Select all
            </Button>
            {selectedIds.size > 0 ? (
              <Button variant="outline" size="xs" onClick={clearSelection}>
                Clear
              </Button>
            ) : null}
          </div>
        )}
      </div>
      <div
        ref={parentRef}
        role="region"
        aria-label="Mailbox messages"
        className="min-h-0 flex-1 overflow-auto"
        data-active-pane={activePane === "mailbox" ? "true" : undefined}
        data-testid="mailbox-list"
        onMouseDown={() => setActivePane("mailbox")}
      >
        <div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>
          {virtualItems.map((virtualItem) => {
            const item = flat[virtualItem.index];
            if (!item) return null;
            return (
              <div
                key={item.kind === "header" ? item.id : item.row.id}
                data-index={virtualItem.index}
                ref={virtualizer.measureElement}
                className="absolute left-0 top-0 w-full"
                style={{ transform: `translateY(${virtualItem.start}px)` }}
              >
                {item.kind === "header" ? (
                  <div className="mailbox-group-header sticky top-0 z-[1] flex h-8 items-center border-b border-border bg-background/95 px-3 font-mono text-2xs uppercase tracking-wide text-muted-foreground backdrop-blur">
                    {item.label}
                  </div>
                ) : (
                  <MailboxRow
                    row={item.row}
                    selected={!readOnly && selectedIds.has(item.row.id)}
                    focused={focusedRow?.id === item.row.id}
                    onToggleSelection={(shift) => toggleRow(item.row, shift)}
                    onFocusPane={() => setActivePane("mailbox")}
                    onOpen={() => openRow(item.row, "mailbox")}
                    readOnly={readOnly}
                    trailingAction={rowAction?.(item.row)}
                  />
                )}
              </div>
            );
          })}
        </div>
      </div>
      {readOnly ? null : <BulkActionBar />}
    </div>
  );
}

function flatten(groups: MessageGroupView[]): FlatItem[] {
  const items: FlatItem[] = [];
  for (const group of groups) {
    items.push({ kind: "header", id: group.id, label: group.label });
    for (const row of group.rows) items.push({ kind: "row", row });
  }
  return items;
}

function mailboxSegment(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[1] && parts[1] !== "label" && parts[1] !== "saved" ? parts[1] : "inbox";
}
