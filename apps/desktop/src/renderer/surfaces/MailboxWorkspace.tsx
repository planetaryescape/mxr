import type {
  LayoutMode,
  MailboxPayload,
  ReaderMode,
  ThreadResponse,
  UtilityRailPayload,
} from "../../shared/types";
import { Filter, RefreshCw, X } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { cn } from "../lib/cn";
import { MailRow, DateGroupHeader } from "../components/MailRow";
import { mailboxRowSelectionId } from "../lib/mailboxSelection";
import { SkeletonMailList } from "../lib/skeleton";
import { ReaderPane } from "./ReaderPane";
import type { FlattenedEntry } from "./types";

export function MailboxWorkspace(props: {
  mailbox: MailboxPayload;
  rows: FlattenedEntry[];
  mailListMode: "threads" | "messages";
  onMailListModeChange: (mode: "threads" | "messages") => void;
  selectedThreadId: string | null;
  selectedMessageIds: Set<string>;
  pendingMessageIds: Set<string>;
  loadingLabel: string | null;
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
  remoteContentEnabled: boolean;
  setRemoteContentEnabled: (value: boolean) => void;
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
      `[data-row-id="${props.selectedThreadId}"]`,
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
        aria-busy={props.loadingLabel ? "true" : "false"}
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
          <div className="flex items-center gap-2">
            {props.loadingLabel ? (
              <span
                role="status"
                aria-live="polite"
                className="inline-flex items-center gap-1 border border-accent/30 bg-accent/10 px-1.5 py-0.5 text-[length:var(--text-xs)] text-accent"
                style={{ borderRadius: "var(--radius-sm)" }}
              >
                <RefreshCw className="size-3 animate-spin" />
                <span>{`Loading ${props.loadingLabel}...`}</span>
              </span>
            ) : null}
            <div
              className="flex border border-outline bg-canvas-elevated p-0.5"
              style={{ borderRadius: "var(--radius-sm)" }}
            >
              {(["threads", "messages"] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  aria-pressed={props.mailListMode === mode}
                  className={cn(
                    "px-2 py-0.5 text-[length:var(--text-xs)] uppercase transition-colors",
                    props.mailListMode === mode
                      ? "bg-accent/12 text-accent"
                      : "text-foreground-subtle hover:text-foreground",
                  )}
                  style={{ borderRadius: "var(--radius-sm)" }}
                  onClick={() => props.onMailListModeChange(mode)}
                >
                  {mode === "threads" ? "Threads" : "Messages"}
                </button>
              ))}
            </div>
            <span className="font-mono text-[length:var(--text-xs)] tabular-nums text-foreground-subtle">
              {props.mailbox.counts.total}
            </span>
          </div>
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
                  selected={
                    props.selectedThreadId === mailboxRowSelectionId(entry.row)
                  }
                  multiSelected={props.selectedMessageIds.has(entry.row.id)}
                  pending={props.pendingMessageIds.has(entry.row.id)}
                  removing={props.removingIds?.has(entry.row.id) ?? false}
                  onSelect={() => props.onSelect(mailboxRowSelectionId(entry.row))}
                  onOpen={props.onOpen}
                  onContextMenu={(e) =>
                    props.onRowContextMenu?.(e, mailboxRowSelectionId(entry.row))
                  }
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
          remoteContentEnabled={props.remoteContentEnabled}
          setRemoteContentEnabled={props.setRemoteContentEnabled}
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
          remoteContentEnabled={props.remoteContentEnabled}
          setRemoteContentEnabled={props.setRemoteContentEnabled}
          signatureExpanded={props.signatureExpanded}
          onArchive={props.onArchive}
          onCloseReader={props.onCloseReader}
        />
      ) : null}
    </div>
  );
}
