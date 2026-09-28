import { Outlet, useNavigate, useParams } from "@tanstack/react-router";
import { Filter, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { MailboxList, type MailboxListProps } from "./MailboxList";
import { Centered, ListSkeleton } from "./MailViewParts";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { ReaderNavContext, type ReaderNav } from "./readerNav";
import type { MessageGroupView, MessageRowView } from "./types";
import { Button } from "@/components/ui/button";
import { SINGLE_PANE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { focusActivePane } from "@/lib/keys/focusPane";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

export interface ListStatus {
  isLoading: boolean;
  isError: boolean;
  error: Error | null;
  refetch: () => unknown;
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
> {
  /** URL of the list; an open conversation lives at `${basePath}/<thread>`. */
  basePath: string;
  title: string;
  meta?: ReactNode;
  actions?: ReactNode;
  /** Extra header content under the title row (filters, tabs). */
  toolbar?: ReactNode;
  /**
   * Replaces the title row with a page heading of its own (the desk's
   * greeting). `title` still names the list for assistive tech.
   */
  heading?: ReactNode;
  /** A line under the list, outside the listbox (the desk's everything-else links). */
  footer?: ReactNode;
  groups: MessageGroupView[];
  scopeKey: string;
  status: ListStatus;
  /** Keep the list's query string when a conversation opens (search). */
  preserveSearch?: boolean;
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
  meta,
  actions,
  toolbar,
  heading,
  footer,
  groups,
  scopeKey,
  status,
  empty,
  preserveSearch = false,
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
  const nav = useMemo<ReaderNav>(
    () => ({
      // Step through what the list shows, so n/N respect the filter.
      threadIds: () => [
        ...new Set(visibleGroups.flatMap((group) => group.rows.map((row) => row.thread_id))),
      ],
      open,
      close,
      queueLabel: listProps.queueLabel,
    }),
    [close, visibleGroups, listProps.queueLabel, open],
  );

  return (
    <ReaderNavContext.Provider value={nav}>
      <div className="flex min-h-0 min-w-0 flex-1">
        <section
          aria-label={title}
          className={cn(
            "@container flex min-h-0 min-w-0 flex-col bg-background",
            threadOpen ? "w-[clamp(340px,36%,480px)] shrink-0 border-r border-border" : "flex-1",
            hideList && "hidden",
          )}
        >
          <header className="shrink-0 border-b border-border">
            {heading ?? (
              <div className="flex h-11 items-center gap-3 px-4">
                <h1 className="truncate text-[15px] font-semibold tracking-tight">{title}</h1>
                {meta ? (
                  <span className="truncate font-mono text-2xs text-muted-foreground tabular-nums">
                    {meta}
                  </span>
                ) : null}
                <span className="ml-auto flex items-center gap-0.5">{actions}</span>
              </div>
            )}
            {toolbar ? <div className="px-4 pb-2.5">{toolbar}</div> : null}
            {filterOpen ? (
              <div className="flex items-center gap-2 border-t border-border px-4 py-2">
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
                  className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-muted-foreground"
                />
                <span className="shrink-0 font-mono text-2xs text-muted-foreground tabular-nums">
                  {filter ? `${shownCount} of ${loadedCount}` : ""}
                </span>
                {filter.trim() ? (
                  <button
                    type="button"
                    onClick={() => void navigate({ to: "/search", search: { q: filter.trim() } })}
                    className="shrink-0 text-[12px] text-primary hover:underline"
                    title="Search all mail (⌘Enter)"
                  >
                    Search all mail
                  </button>
                ) : null}
              </div>
            ) : null}
          </header>
          {phase !== "ready" ? (
            <ListSkeleton quiet={phase === "quiet"} />
          ) : status.isError ? (
            <Centered
              icon={<RefreshCw className="size-6" />}
              title={`Couldn't load ${title.toLowerCase()}`}
              body={status.error?.message}
              action={
                <Button size="sm" onClick={() => void status.refetch()}>
                  Try again
                </Button>
              }
            />
          ) : (
            <MailboxList
              {...listProps}
              groups={visibleGroups}
              scopeKey={scopeKey}
              onFilter={openFilter}
              label={title}
              activeThreadId={threadId}
              previewOnFocus={threadOpen && !hideList}
              onOpenRow={onOpenRow}
              onCloseThread={threadOpen ? close : undefined}
              empty={
                filter.trim() && loadedCount > 0 ? (
                  <Centered
                    icon={<Filter className="size-6" />}
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
        {threadOpen ? <Outlet /> : null}
      </div>
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
