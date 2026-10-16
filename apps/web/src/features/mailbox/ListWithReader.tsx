import { Outlet, useNavigate, useParams } from "@tanstack/react-router";
import { Filter } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { LIST_PANE_SIZE } from "./listPane";
import { MailboxList, type MailboxListProps } from "./MailboxList";
import { useAdvanceOnRemoval, useRouteStillShows } from "@/hooks/useAdvanceOnRemoval";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { ReaderNavContext, type ReaderNav } from "./readerNav";
import type { MessageGroupView, MessageRowView } from "./types";
import { DaemonUnavailableError } from "@/api/client";
import { ModeFrame } from "@/components/ModeFrame";
import { PageEmpty, PageError, PageSkeleton } from "@/components/PageParts";
import {
  FramePanesContext,
  PageFrame,
  READER_TOOLBAR_HEIGHT,
  type PageAction,
} from "@/components/PageFrame";
import { Button } from "@/components/ui/button";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { SINGLE_PANE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { useSplitPane } from "@/hooks/useSplitPane";
import { focusActivePane } from "@/lib/keys/focusPane";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

export interface ListStatus {
  isLoading: boolean;
  isError: boolean;
  error: Error | null;
  refetch: () => unknown;
  fetchStatus?: "fetching" | "paused" | "idle";
  data?: unknown;
}

interface ListWithReaderProps extends Pick<
  MailboxListProps,
  | "hasMore"
  | "loadingMore"
  | "onLoadMore"
  | "readOnly"
  | "rowAction"
  | "queueLabel"
  | "empty"
  | "renderRow"
  | "airyHeaders"
  | "interceptVerb"
  | "swipeActions"
  | "rowGists"
  | "modeChips"
> {
  /** URL of the list; an open conversation lives at `${basePath}/<thread>`. */
  basePath: string;
  title: string;
  /** The title as rendered, when it holds links (the desk's counts). `title` still names the list. */
  titleContent?: ReactNode;
  /** One line saying what this list is for. */
  purpose?: ReactNode;
  /** A count or short label above the title ("12 conversations"). */
  meta?: ReactNode;
  primary?: ReactNode;
  overflow?: readonly PageAction[];
  /** Filters and notes between the page frame and the list. */
  toolbar?: ReactNode;
  /** A line under the list, outside the listbox (the desk's everything-else links). */
  footer?: ReactNode;
  groups: MessageGroupView[];
  scopeKey: string;
  status: ListStatus;
  /** Keep the list's query string when a conversation opens (search). */
  preserveSearch?: boolean;
  /** What `g f` does instead of filtering the loaded rows (search opens its own filters). */
  onFilterKey?: () => void;
}

/**
 * A list of conversations with the reader beside it (or in its place on
 * narrow screens and in full-width mode). Mail lenses, the reply queue,
 * owed replies and snoozed mail all share this, so opening, closing and
 * stepping through conversations behaves the same everywhere.
 */
export function ListWithReader({
  basePath,
  title,
  titleContent,
  purpose,
  meta,
  primary,
  overflow,
  toolbar,
  footer,
  groups,
  scopeKey,
  status,
  empty,
  preserveSearch = false,
  onFilterKey,
  ...listProps
}: ListWithReaderProps) {
  const navigate = useNavigate();
  const params = useParams({ strict: false }) as { threadId?: string };
  const threadId = params.threadId;
  const singlePane = useMediaQuery(SINGLE_PANE_QUERY);
  const readerLayout = useUiPrefs((s) => s.readerLayout);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const threadOpen = Boolean(threadId);
  const phase = useDelayedPending(status.isLoading);
  const hasData = status.data !== undefined || groups.length > 0;
  const [filterOpen, setFilterOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const filterRef = useRef<HTMLInputElement>(null);
  // Leaving the list (another lens, another query) drops the filter.
  useEffect(() => {
    setFilter("");
    setFilterOpen(false);
  }, [scopeKey]);
  const visibleGroups = useMemo(() => filterGroups(groups, filter), [filter, groups]);
  const loadedCount = useMemo(
    () => groups.reduce((sum, group) => sum + group.rows.length, 0),
    [groups],
  );
  const shownCount = useMemo(
    () => visibleGroups.reduce((sum, group) => sum + group.rows.length, 0),
    [visibleGroups],
  );
  const openFilter = useCallback(() => {
    setFilterOpen(true);
    requestAnimationFrame(() => filterRef.current?.select());
  }, []);
  const hideList = threadOpen && (singlePane || readerLayout === "full");
  const split = threadOpen && !hideList;
  const panes = useMemo(() => ({ split, inset: READER_TOOLBAR_HEIGHT }), [split]);
  // One saved width for every mail list, so Inbox, labels and search match.
  const pane = useSplitPane("mail-list", LIST_PANE_SIZE, { active: split });

  // When the conversation closes by any route (Esc, back button), hand the
  // keyboard back to the list. Keyed on the transition only: opening sets
  // the reader pane before the route has changed.
  useEffect(() => {
    if (!threadOpen && useMailboxPane.getState().activePane === "reader") setActivePane("mailbox");
  }, [setActivePane, threadOpen]);

  const open = useCallback(
    (id: string, options?: { focusReader?: boolean }) => {
      setActivePane(options?.focusReader === false ? "mailbox" : "reader");
      void navigate({
        to: `${basePath}/${encodeURIComponent(id)}`,
        search: preserveSearch ? (previous: Record<string, unknown>) => previous : undefined,
      });
    },
    [basePath, navigate, preserveSearch, setActivePane],
  );
  const close = useCallback(() => {
    setActivePane("mailbox");
    void navigate({
      to: basePath,
      search: preserveSearch ? (previous: Record<string, unknown>) => previous : undefined,
    });
  }, [basePath, navigate, preserveSearch, setActivePane]);
  const onOpenRow = useCallback(
    (row: MessageRowView, options: { focusReader: boolean }) => open(row.thread_id, options),
    [open],
  );
  // The open conversation left the list from somewhere other than the
  // reader (the list's own archive, sync): open its neighbour. The list
  // as loaded, not as filtered, so typing a filter never moves the reader.
  const loadedIds = useMemo(
    () => [...new Set(groups.flatMap((group) => group.rows.map((row) => row.thread_id)))],
    [groups],
  );
  const loadedIdsRef = useRef(loadedIds);
  loadedIdsRef.current = loadedIds;
  const isListed = useCallback((id: string) => loadedIdsRef.current.includes(id), []);
  const stillShows = useRouteStillShows();
  const expectRemoval = useAdvanceOnRemoval({
    ids: loadedIds,
    ready: !status.isLoading,
    selectedId: threadId ?? null,
    onAdvance: (next, from) => {
      if (from !== null && !stillShows(from)) return;
      open(next, { focusReader: useMailboxPane.getState().activePane === "reader" });
    },
    onEmpty: (removed) => {
      if (stillShows(removed)) close();
    },
  });
  const nav = useMemo<ReaderNav>(
    () => ({
      // Step through what the list shows, so n/N respect the filter.
      threadIds: () => [
        ...new Set(visibleGroups.flatMap((group) => group.rows.map((row) => row.thread_id))),
      ],
      open,
      close,
      expectRemoval,
      isListed,
      queueLabel: listProps.queueLabel,
      intercept: listProps.interceptVerb,
    }),
    [
      close,
      expectRemoval,
      visibleGroups,
      isListed,
      listProps.queueLabel,
      listProps.interceptVerb,
      open,
    ],
  );

  return (
    <ReaderNavContext.Provider value={nav}>
      <FramePanesContext.Provider value={panes}>
        <ResizablePanelGroup className="relative min-h-0 min-w-0 flex-1" {...pane.groupProps}>
          <ResizablePanel
            id="list-pane"
            data-parked={hideList || undefined}
            {...pane.sidePanelProps}
            className="flex min-h-0 flex-col"
          >
            <section
              aria-label={title}
              className="@container flex min-h-0 min-w-0 flex-1 flex-col bg-background"
            >
              <PageFrame
                title={title}
                titleContent={titleContent}
                eyebrow={meta}
                purpose={purpose}
                primary={primary}
                overflow={overflow}
              />
              {toolbar ? (
                <div className="shrink-0 border-b border-rule-soft">
                  <ModeFrame className="px-4 py-2">{toolbar}</ModeFrame>
                </div>
              ) : null}
              {filterOpen ? (
                <div className="flex shrink-0 items-center gap-2 border-b border-rule-soft px-4 py-2">
                  <Filter className="size-3.5 shrink-0 text-muted-foreground" />
                  <input
                    ref={filterRef}
                    aria-label="Filter this list"
                    value={filter}
                    onChange={(event) => setFilter(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") {
                        event.preventDefault();
                        setFilter("");
                        setFilterOpen(false);
                        focusActivePane();
                      } else if (
                        event.key === "Enter" &&
                        (event.metaKey || event.ctrlKey) &&
                        filter.trim()
                      ) {
                        event.preventDefault();
                        void navigate({ to: "/search", search: { q: filter.trim() } });
                      } else if (event.key === "Enter" || event.key === "ArrowDown") {
                        event.preventDefault();
                        setActivePane("mailbox");
                        focusActivePane();
                      }
                    }}
                    placeholder="Filter loaded conversations by sender, subject or snippet"
                    className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
                  />
                  <span className="shrink-0 font-mono text-2xs text-muted-foreground tabular-nums">
                    {filter ? `${shownCount} of ${loadedCount}` : ""}
                  </span>
                  {filter.trim() ? (
                    <button
                      type="button"
                      onClick={() => void navigate({ to: "/search", search: { q: filter.trim() } })}
                      className="shrink-0 text-xs text-primary hover:underline"
                      title="Search all mail (⌘Enter)"
                    >
                      Search all mail
                    </button>
                  ) : null}
                </div>
              ) : null}
              {phase !== "ready" ? (
                <PageSkeleton
                  framed
                  lead="avatar"
                  shown={phase === "skeleton"}
                  label={`Loading ${title}`}
                />
              ) : (status.fetchStatus === "paused" || status.isError) && !hasData ? (
                <PageError
                  framed
                  title={`Couldn't load ${title}`}
                  error={status.error ?? new DaemonUnavailableError()}
                  onRetry={() => void status.refetch()}
                />
              ) : (
                <MailboxList
                  {...listProps}
                  groups={visibleGroups}
                  scopeKey={scopeKey}
                  onFilter={onFilterKey ?? openFilter}
                  label={title}
                  activeThreadId={threadId}
                  previewOnFocus={threadOpen && !hideList}
                  onOpenRow={onOpenRow}
                  onCloseThread={threadOpen ? close : undefined}
                  empty={
                    filter.trim() && loadedCount > 0 ? (
                      <PageEmpty
                        framed
                        title="Nothing loaded matches"
                        body={`No conversation in this list mentions “${filter.trim()}”.`}
                        action={
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() =>
                              void navigate({ to: "/search", search: { q: filter.trim() } })
                            }
                          >
                            Search all mail
                          </Button>
                        }
                      />
                    ) : (
                      empty
                    )
                  }
                />
              )}
              {footer}
            </section>
          </ResizablePanel>
          {/* The handle and reader stay mounted, parked off screen while
            unused (app.css): registering either with the library on open
            reads layout mid-commit, which pushed opening a conversation
            past the speed gate. */}
          <ResizableHandle
            aria-label={`Resize ${title} list`}
            data-parked={!split || undefined}
            {...pane.handleProps}
          />
          <ResizablePanel
            id="reader-pane"
            data-parked={!threadOpen || undefined}
            {...pane.otherPanelProps}
            className="flex min-h-0 min-w-0"
          >
            {threadOpen ? <Outlet /> : null}
          </ResizablePanel>
        </ResizablePanelGroup>
      </FramePanesContext.Provider>
    </ReaderNavContext.Provider>
  );
}

/**
 * TUI Ctrl-f: an instant filter over what is already loaded. Every word must
 * appear in the sender, subject or snippet; a full search is one key away.
 */
function filterGroups(groups: MessageGroupView[], filter: string): MessageGroupView[] {
  const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return groups;
  return groups
    .map((group) => ({
      ...group,
      rows: group.rows.filter((row) => {
        const haystack =
          `${row.sender} ${row.sender_detail ?? ""} ${row.subject} ${row.snippet}`.toLowerCase();
        return words.every((word) => haystack.includes(word));
      }),
    }))
    .filter((group) => group.rows.length > 0);
}
