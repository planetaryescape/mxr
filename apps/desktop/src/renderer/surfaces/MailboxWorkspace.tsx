import type {
  LayoutMode,
  MailboxPayload,
  ReaderMode,
  ThreadResponse,
  UtilityRailPayload,
} from "../../shared/types";
import { Filter, X } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { cn } from "../lib/cn";
import { MailRow, DateGroupHeader } from "../components/MailRow";
import { SkeletonMailList } from "../lib/skeleton";
import { ReaderPane } from "./ReaderPane";
import type { FlattenedEntry } from "./types";

export function MailboxWorkspace(props: {
  mailbox: MailboxPayload;
  rows: FlattenedEntry[];
  mailListMode: "threads" | "messages";
  selectedThreadId: string | null;
  selectedMessageIds: Set<string>;
  pendingMessageIds: Set<string>;
  removingIds?: Set<string>;
  filterQuery: string;
  filterOpen: boolean;
  onFilterChange: (query: string) => void;
  onFilterClose: () => void;
  onSelect: (threadId: string) => void;
  onOpen: () => void;
  onRowContextMenu?: (e: React.MouseEvent, threadId: string) => void;
  layoutMode: LayoutMode;
  thread: ThreadResponse | null;
  readerMode: ReaderMode;
  setReaderMode: (mode: ReaderMode) => void;
  signatureExpanded: boolean;
  onArchive: () => void;
  onCloseReader: () => void;
  utilityRail: UtilityRailPayload;
}) {
  const showReader = props.layoutMode !== "twoPane";
  const listRef = useRef<HTMLElement | null>(null);
  const filterRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!props.selectedThreadId) {
      return;
    }
    const selectedRow = listRef.current?.querySelector<HTMLElement>(
      `[data-thread-id="${props.selectedThreadId}"]`,
    );
    if (selectedRow && typeof selectedRow.scrollIntoView === "function") {
      selectedRow.scrollIntoView({ block: "nearest" });
    }
  }, [props.rows, props.selectedThreadId]);

  // Client-side filter
  const filteredRows = useMemo(() => {
    if (!props.filterOpen || !props.filterQuery) return props.rows;
    const q = props.filterQuery.toLowerCase();
    return props.rows.filter((entry) => {
      if (entry.kind === "header") return true;
      return (
        entry.row.sender.toLowerCase().includes(q) ||
        entry.row.subject.toLowerCase().includes(q) ||
        entry.row.snippet.toLowerCase().includes(q)
      );
    });
  }, [props.rows, props.filterOpen, props.filterQuery]);

  // Auto-focus filter input when opened
  useEffect(() => {
    if (props.filterOpen && filterRef.current) {
      filterRef.current.focus();
    }
  }, [props.filterOpen]);

  const displayRows = filteredRows;
  const hasRows = displayRows.length > 0;

  return (
    <div
      className={cn(
        "grid h-full min-h-0 grid-cols-1",
        props.layoutMode === "threePane" ? "xl:grid-cols-[minmax(20rem,0.84fr)_minmax(0,1fr)]" : "",
      )}
    >
      <section
        ref={listRef}
        className={cn(
          "subtle-scrollbar flex min-h-0 flex-col border-r border-outline bg-panel",
          props.layoutMode === "fullScreen" ? "hidden" : "",
        )}
      >
        {/* Compact list header */}
        <div className="flex items-center justify-between gap-2 border-b border-outline px-3 py-2">
          <div className="flex items-center gap-2">
            <h1 className="text-[length:var(--text-base)] font-semibold text-foreground">
              {props.mailbox.lensLabel}
            </h1>
            {props.mailbox.counts.unread > 0 ? (
              <span className="font-mono text-[length:var(--text-xs)] tabular-nums text-accent">
                {props.mailbox.counts.unread}
              </span>
            ) : null}
          </div>
          <span className="font-mono text-[length:var(--text-xs)] tabular-nums text-foreground-subtle">
            {props.mailbox.counts.total}
          </span>
        </div>

        {/* Inline filter bar */}
        {props.filterOpen ? (
          <div className="flex items-center gap-2 border-b border-outline bg-canvas-elevated px-3 py-1.5">
            <Filter className="size-3 text-foreground-subtle" />
            <input
              ref={filterRef}
              className="min-w-0 flex-1 bg-transparent text-[length:var(--text-sm)] text-foreground outline-none placeholder:text-foreground-subtle"
              value={props.filterQuery}
              onChange={(e) => props.onFilterChange(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.preventDefault();
                  props.onFilterClose();
                }
              }}
              placeholder="Filter by sender, subject, snippet..."
            />
            <span className="text-[length:var(--text-xs)] text-foreground-subtle">
              {displayRows.filter((e) => e.kind === "row").length}
            </span>
            <button
              className="flex size-5 items-center justify-center text-foreground-subtle hover:text-foreground"
              onClick={props.onFilterClose}
            >
              <X className="size-3" />
            </button>
          </div>
        ) : null}

        {/* Mail list */}
        <div className="subtle-scrollbar min-h-0 flex-1 overflow-y-auto">
        {hasRows ? (
          <div>
            {displayRows.map((entry) =>
              entry.kind === "header" ? (
                <DateGroupHeader key={entry.id} label={entry.label} />
              ) : (
                <MailRow
                  key={entry.id}
                  row={entry.row}
                  selected={props.selectedThreadId === entry.row.thread_id}
                  multiSelected={props.selectedMessageIds.has(entry.row.id)}
                  pending={props.pendingMessageIds.has(entry.row.id)}
                  removing={props.removingIds?.has(entry.row.id) ?? false}
                  onSelect={() => props.onSelect(entry.row.thread_id)}
                  onOpen={props.onOpen}
                  onContextMenu={(e) => props.onRowContextMenu?.(e, entry.row.thread_id)}
                />
              ),
            )}
          </div>
        ) : (
          <SkeletonMailList />
        )}
        </div>
      </section>

      {showReader ? (
        <ReaderPane
          className={cn(
            props.layoutMode === "fullScreen" ? "min-h-0 flex" : "hidden min-h-0 xl:flex",
            props.layoutMode === "fullScreen" ? "xl:col-span-2" : "",
          )}
          thread={props.thread}
          readerMode={props.readerMode}
          setReaderMode={props.setReaderMode}
          signatureExpanded={props.signatureExpanded}
          onArchive={props.onArchive}
          onCloseReader={props.onCloseReader}
        />
      ) : null}

      {showReader && props.layoutMode !== "fullScreen" ? (
        <ReaderPane
          className="fixed inset-y-12 right-0 z-10 flex w-[min(100vw-4rem,32rem)] border-l border-outline bg-panel xl:hidden"
          thread={props.thread}
          readerMode={props.readerMode}
          setReaderMode={props.setReaderMode}
          signatureExpanded={props.signatureExpanded}
          onArchive={props.onArchive}
          onCloseReader={props.onCloseReader}
        />
      ) : null}
    </div>
  );
}
