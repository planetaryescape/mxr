import { Tabs } from "@base-ui/react";
import { MailSearch } from "lucide-react";
import type { RefObject } from "react";
import type {
  ReaderMode,
  SearchMode,
  SearchResponse,
  SearchScope,
  SearchSort,
  ThreadResponse,
} from "../../shared/types";
import { cn } from "../lib/cn";
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
  thread: ThreadResponse | null;
  readerMode: ReaderMode;
  setReaderMode: (mode: ReaderMode) => void;
  signatureExpanded: boolean;
}) {
  return (
    <div className="grid h-full min-h-0 grid-cols-1 xl:grid-cols-[minmax(22rem,0.92fr)_minmax(32rem,1.15fr)]">
      <section className="min-h-0 border-r border-outline bg-panel">
        <div className="border-b border-outline px-4 py-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <p className="mono-meta">Search</p>
              <h1 className="mt-1.5 text-balance text-[1.55rem] font-semibold leading-none text-foreground">
                Search local mail
              </h1>
            </div>
            <div className="flex items-center gap-2">
              <select
                aria-label="Search mode"
                className="rounded-lg border border-outline bg-canvas-elevated px-2.5 py-1.5 text-[12px] text-foreground-muted outline-none"
                value={props.mode}
                onChange={(event) => props.onModeChange(event.target.value as SearchMode)}
              >
                <option value="lexical">Lexical</option>
                <option value="hybrid">Hybrid</option>
                <option value="semantic">Semantic</option>
              </select>
              <select
                aria-label="Sort"
                className="rounded-lg border border-outline bg-canvas-elevated px-2.5 py-1.5 text-[12px] text-foreground-muted outline-none"
                value={props.sort}
                onChange={(event) => props.onSortChange(event.target.value as SearchSort)}
              >
                <option value="relevant">Relevant</option>
                <option value="recent">Recent</option>
              </select>
            </div>
          </div>
          <div className="mt-4 flex items-center gap-3 rounded-xl border border-outline bg-canvas-elevated px-3 py-2.5">
            <MailSearch className="size-4 text-foreground-subtle" />
            <input
              ref={props.inputRef}
              className="min-w-0 flex-1 bg-transparent text-[13px] text-foreground outline-none placeholder:text-foreground-subtle"
              value={props.query}
              onChange={(event) => props.onQueryChange(event.target.value)}
              placeholder="Search subjects, senders, snippets"
            />
          </div>
          <Tabs.Root
            value={props.scope}
            onValueChange={(value) => props.onScopeChange((value ?? "threads") as SearchScope)}
            className="mt-5"
          >
            <Tabs.List className="flex gap-2">
              <Tabs.Tab
                value="threads"
                className="rounded-lg border border-outline px-2.5 py-1.5 text-[12px] text-foreground-muted data-[selected]:border-accent/35 data-[selected]:bg-accent/10 data-[selected]:text-foreground"
              >
                Threads
              </Tabs.Tab>
              <Tabs.Tab
                value="messages"
                className="rounded-lg border border-outline px-2.5 py-1.5 text-[12px] text-foreground-muted data-[selected]:border-accent/35 data-[selected]:bg-accent/10 data-[selected]:text-foreground"
              >
                Messages
              </Tabs.Tab>
              <Tabs.Tab
                value="attachments"
                className="rounded-lg border border-outline px-2.5 py-1.5 text-[12px] text-foreground-muted data-[selected]:border-accent/35 data-[selected]:bg-accent/10 data-[selected]:text-foreground"
              >
                Attachments
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.Root>
          <div className="mt-4 flex items-center justify-between gap-3 rounded-xl border border-outline bg-panel-muted px-3 py-2.5">
            <div>
              <p className="mono-meta">Ranking</p>
              <p className="mt-1.5 text-[12px] text-foreground-muted">
                {props.mode} mode · {props.sort} sort
              </p>
            </div>
            <label className="flex items-center gap-2 text-[12px] text-foreground-muted">
              <input
                type="checkbox"
                checked={props.explain}
                onChange={(event) => props.onExplainChange(event.target.checked)}
              />
              Explain
            </label>
          </div>
        </div>
        <div className="subtle-scrollbar h-[calc(100%-11.5rem)] overflow-y-auto px-4 py-4">
          <p className="mono-meta">{props.state.total} results</p>
          {props.explain ? (
            <div className="mt-3 rounded-xl border border-outline bg-panel-muted px-3 py-3">
              <p className="mono-meta">Explain</p>
              <pre className="mt-2 whitespace-pre-wrap text-[11px] leading-5 text-foreground-muted">
                {props.state.explain
                  ? JSON.stringify(props.state.explain, null, 2)
                  : "No explain payload for this query."}
              </pre>
            </div>
          ) : null}
          <div className="mt-4 space-y-1.5">
            {props.rows.map((entry) =>
              entry.kind === "header" ? (
                <div key={entry.id} className="mono-meta border-b border-outline/60 pb-2 pt-4 first:pt-0">
                  {entry.label}
                </div>
              ) : (
                <button
                  key={entry.id}
                  className={cn(
                    "w-full rounded-xl border px-3 py-2.5 text-left transition-colors",
                    props.selectedThreadId === entry.row.thread_id
                      ? "border-accent/35 bg-panel-elevated"
                      : props.selectedMessageIds.has(entry.row.id)
                        ? "border-success/35 bg-success/10"
                        : "border-transparent bg-transparent hover:bg-panel-elevated/72",
                  )}
                  onClick={() => props.onSelect(entry.row.thread_id)}
                  onDoubleClick={props.onOpen}
                >
                  <div className="flex items-start justify-between gap-4">
                    <div className="min-w-0">
                      <div className="truncate text-[13px] font-medium text-foreground">
                        {entry.row.subject}
                      </div>
                      <p className="mt-1 text-[12px] text-foreground-muted">{entry.row.sender}</p>
                      <p className="mt-1 line-clamp-1 text-[12px] leading-5 text-foreground-subtle">
                        {entry.row.snippet}
                      </p>
                    </div>
                    <div className="flex shrink-0 flex-col items-end gap-2">
                      {props.pendingMessageIds.has(entry.row.id) ? (
                        <span className="rounded-full border border-accent/30 bg-accent/10 px-2 py-0.5 text-[10px] text-accent">
                          Syncing
                        </span>
                      ) : null}
                      <span className="font-mono text-[10px] tabular-nums text-foreground-subtle">
                        {entry.row.date_label}
                      </span>
                    </div>
                  </div>
                </button>
              ),
            )}
          </div>
        </div>
      </section>
      <ReaderPane
        className="hidden min-h-0 xl:flex"
        thread={props.thread}
        readerMode={props.readerMode}
        setReaderMode={props.setReaderMode}
        signatureExpanded={props.signatureExpanded}
        onArchive={() => undefined}
        onCloseReader={() => undefined}
      />
    </div>
  );
}
