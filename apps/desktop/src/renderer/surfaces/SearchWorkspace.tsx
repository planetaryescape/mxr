import { Tabs } from "@base-ui/react";
import { Download, ExternalLink, MailSearch, Paperclip } from "lucide-react";
import { useEffect, useRef, type RefObject } from "react";
import type {
  LayoutMode,
  MailboxRow,
  ReaderMode,
  SearchMode,
  SearchResponse,
  SearchScope,
  SearchSort,
  ThreadResponse,
} from "../../shared/types";
import { cn } from "../lib/cn";
import { MailRow, DateGroupHeader } from "../components/MailRow";
import { mailboxRowSelectionId } from "../lib/mailboxSelection";
import { ReaderPane } from "./ReaderPane";
import type { FlattenedEntry } from "./types";

export function SearchWorkspace(props: {
  inputRef: RefObject<HTMLInputElement | null>;
  query: string;
  onQueryChange: (value: string) => void;
  scope: SearchScope;
  onScopeChange: (value: SearchScope) => void;
  mode: SearchMode;
  onModeChange: (value: SearchMode) => void;
  sort: SearchSort;
  onSortChange: (value: SearchSort) => void;
  explain: boolean;
  onExplainChange: (value: boolean) => void;
  state: SearchResponse;
  rows: FlattenedEntry[];
  selectedMessageIds: Set<string>;
  pendingMessageIds: Set<string>;
  selectedThreadId: string | null;
  onSelect: (threadId: string) => void;
  onOpen: () => void;
  layoutMode: LayoutMode;
  thread: ThreadResponse | null;
  readerMode: ReaderMode;
  setReaderMode: (mode: ReaderMode) => void;
  remoteContentEnabled: boolean;
  setRemoteContentEnabled: (value: boolean) => void;
  signatureExpanded: boolean;
  onArchive: () => void;
  onCloseReader: () => void;
  onLoadMore?: () => void;
  onOpenAttachment?: (attachmentId: string, messageId: string) => void;
  onDownloadAttachment?: (attachmentId: string, messageId: string) => void;
}) {
  const resultsRef = useRef<HTMLDivElement | null>(null);
  const showReader = props.layoutMode !== "twoPane";

  useEffect(() => {
    if (!props.selectedThreadId) {
      return;
    }
    const selectedRow = resultsRef.current?.querySelector<HTMLElement>(
      `[data-row-id="${props.selectedThreadId}"]`,
    );
    if (selectedRow && typeof selectedRow.scrollIntoView === "function") {
      selectedRow.scrollIntoView({ block: "nearest" });
    }
  }, [props.rows, props.selectedThreadId]);

  const scopeTabClass = cn(
    "px-2 py-1 text-[length:var(--text-xs)] uppercase text-foreground-subtle",
    "data-[selected]:bg-accent/12 data-[selected]:text-accent",
  );

  return (
    <div
      className={cn(
        "grid h-full min-h-0 grid-cols-1",
        props.layoutMode === "threePane"
          ? "xl:grid-cols-[minmax(22rem,0.92fr)_minmax(32rem,1.15fr)]"
          : "",
      )}
    >
      <section
        className={cn(
          "flex min-h-0 flex-col border-r border-outline bg-panel",
          props.layoutMode === "fullScreen" ? "hidden" : "",
        )}
      >
        {/* Search header */}
        <div className="border-b border-outline px-3 py-2.5">
          <div className="flex items-center justify-between gap-2">
            <span className="text-[length:var(--text-base)] font-semibold text-foreground">
              Search
            </span>
            <div className="flex items-center gap-1.5">
              <select
                aria-label="Search mode"
                className="border border-outline bg-canvas-elevated px-1.5 py-1 text-[length:var(--text-xs)] text-foreground-muted outline-none"
                style={{ borderRadius: "var(--radius-sm)" }}
                value={props.mode}
                onChange={(event) => props.onModeChange(event.target.value as SearchMode)}
              >
                <option value="lexical">Lexical</option>
                <option value="hybrid">Hybrid</option>
                <option value="semantic">Semantic</option>
              </select>
              <select
                aria-label="Sort"
                className="border border-outline bg-canvas-elevated px-1.5 py-1 text-[length:var(--text-xs)] text-foreground-muted outline-none"
                style={{ borderRadius: "var(--radius-sm)" }}
                value={props.sort}
                onChange={(event) => props.onSortChange(event.target.value as SearchSort)}
              >
                <option value="relevant">Relevant</option>
                <option value="recent">Recent</option>
              </select>
            </div>
          </div>

          {/* Search input */}
          <div
            className="mt-2 flex items-center gap-2 border border-outline bg-canvas-elevated px-2.5 py-2"
            style={{ borderRadius: "var(--radius-sm)" }}
          >
            <MailSearch className="size-3.5 text-foreground-subtle" />
            <input
              ref={props.inputRef}
              className="min-w-0 flex-1 bg-transparent text-[length:var(--text-sm)] text-foreground outline-none placeholder:text-foreground-subtle"
              value={props.query}
              onChange={(event) => props.onQueryChange(event.target.value)}
              placeholder="Search subjects, senders, snippets"
            />
          </div>

          {/* Scope tabs */}
          <Tabs.Root
            value={props.scope}
            onValueChange={(value) => props.onScopeChange((value ?? "threads") as SearchScope)}
            className="mt-2"
          >
            <Tabs.List className="flex gap-0.5">
              <Tabs.Tab value="threads" className={scopeTabClass} style={{ borderRadius: "var(--radius-sm)" }}>
                Threads
              </Tabs.Tab>
              <Tabs.Tab value="messages" className={scopeTabClass} style={{ borderRadius: "var(--radius-sm)" }}>
                Messages
              </Tabs.Tab>
              <Tabs.Tab value="attachments" className={scopeTabClass} style={{ borderRadius: "var(--radius-sm)" }}>
                Attachments
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.Root>

          {/* Ranking info */}
          <div className="mt-2 flex items-center justify-between gap-3 text-[length:var(--text-xs)]">
            <span className="text-foreground-subtle">
              {props.state.total} results · {props.mode} · {props.sort}
            </span>
            <label className="flex items-center gap-1.5 text-foreground-subtle">
              <input
                type="checkbox"
                checked={props.explain}
                onChange={(event) => props.onExplainChange(event.target.checked)}
              />
              Explain
            </label>
          </div>
        </div>

        {/* Results */}
        <div
          ref={resultsRef}
          className="subtle-scrollbar min-h-0 flex-1 overflow-y-auto"
        >
          {props.explain && props.state.explain ? (
            <div className="border-b border-outline bg-panel-muted px-3 py-2">
              <p className="mono-meta">Explain</p>
              <pre className="mt-1.5 whitespace-pre-wrap text-[length:var(--text-xs)] leading-relaxed text-foreground-muted">
                {JSON.stringify(props.state.explain, null, 2)}
              </pre>
            </div>
          ) : null}

          <div>
            {props.rows.map((entry) =>
              entry.kind === "header" ? (
                <DateGroupHeader key={entry.id} label={entry.label} />
              ) : entry.row.kind === "attachment" && entry.row.attachment_id ? (
                <AttachmentSearchRow
                  key={entry.id}
                  row={entry.row}
                  selected={
                    props.selectedThreadId === mailboxRowSelectionId(entry.row)
                  }
                  multiSelected={props.selectedMessageIds.has(entry.row.id)}
                  pending={props.pendingMessageIds.has(entry.row.id)}
                  onSelect={() => props.onSelect(mailboxRowSelectionId(entry.row))}
                  onOpen={props.onOpen}
                  onOpenAttachment={() =>
                    props.onOpenAttachment?.(entry.row.attachment_id!, entry.row.id)
                  }
                  onDownloadAttachment={() =>
                    props.onDownloadAttachment?.(entry.row.attachment_id!, entry.row.id)
                  }
                />
              ) : (
                <MailRow
                  key={entry.id}
                  row={entry.row}
                  selected={
                    props.selectedThreadId === mailboxRowSelectionId(entry.row)
                  }
                  multiSelected={props.selectedMessageIds.has(entry.row.id)}
                  pending={props.pendingMessageIds.has(entry.row.id)}
                  removing={false}
                  onSelect={() => props.onSelect(mailboxRowSelectionId(entry.row))}
                  onOpen={props.onOpen}
                  onContextMenu={() => {}}
                />
              ),
            )}
          </div>
          {props.state.has_more && props.onLoadMore ? (
            <button
              type="button"
              className="w-full border-t border-outline px-3 py-2.5 text-center text-[length:var(--text-xs)] text-accent hover:bg-panel-elevated"
              onClick={props.onLoadMore}
            >
              Load more results
            </button>
          ) : null}
        </div>
      </section>

      {showReader ? (
        <ReaderPane
          className={cn(
            props.layoutMode === "fullScreen"
              ? "min-h-0 flex"
              : "hidden min-h-0 xl:flex",
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

function AttachmentSearchRow(props: {
  row: MailboxRow;
  selected: boolean;
  multiSelected: boolean;
  pending: boolean;
  onSelect: () => void;
  onOpen: () => void;
  onOpenAttachment: () => void;
  onDownloadAttachment: () => void;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      data-testid="mail-row"
      data-row-id={mailboxRowSelectionId(props.row)}
      data-thread-id={props.row.thread_id}
      className={cn(
        "group flex w-full items-start gap-2.5 border-l-2 px-2.5 py-2 text-left transition-colors",
        props.selected
          ? "border-l-accent bg-panel-elevated"
          : props.multiSelected
            ? "border-l-success/60 bg-success/6"
            : "border-l-transparent hover:bg-panel-elevated/50",
      )}
      onClick={props.onSelect}
      onDoubleClick={props.onOpen}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          props.onOpen();
        }
      }}
    >
      <Paperclip className="mt-0.5 size-3.5 shrink-0 text-accent" />
      <div className="min-w-0 flex-1">
        <div className="flex items-center justify-between gap-2">
          <span className="truncate text-[length:var(--text-sm)] font-semibold text-foreground">
            {props.row.attachment_filename ?? props.row.subject}
          </span>
          <span className="shrink-0 font-mono text-[length:var(--text-xs)] tabular-nums text-foreground-subtle">
            {props.pending ? "Syncing" : props.row.date_label}
          </span>
        </div>
        <p className="mt-0.5 truncate text-[length:var(--text-sm)] text-foreground">
          {props.row.sender} · {props.row.snippet}
        </p>
        <div className="mt-2 flex items-center gap-2">
          <button
            type="button"
            className="flex items-center gap-1 border border-outline px-2 py-1 text-[length:var(--text-xs)] text-foreground-muted hover:bg-panel-elevated hover:text-foreground"
            style={{ borderRadius: "var(--radius-sm)" }}
            onClick={(event) => {
              event.stopPropagation();
              props.onOpenAttachment();
            }}
            aria-label={`Open attachment ${props.row.attachment_filename ?? props.row.subject}`}
          >
            <ExternalLink className="size-3" />
            Open
          </button>
          <button
            type="button"
            className="flex items-center gap-1 border border-outline px-2 py-1 text-[length:var(--text-xs)] text-foreground-muted hover:bg-panel-elevated hover:text-foreground"
            style={{ borderRadius: "var(--radius-sm)" }}
            onClick={(event) => {
              event.stopPropagation();
              props.onDownloadAttachment();
            }}
            aria-label={`Download attachment ${props.row.attachment_filename ?? props.row.subject}`}
          >
            <Download className="size-3" />
            Download
          </button>
        </div>
      </div>
    </div>
  );
}
