import { Outlet, useNavigate, useParams } from "@tanstack/react-router";
import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, type ReactNode } from "react";

import { MailboxList, type MailboxListProps } from "./MailboxList";
import { Centered, ListSkeleton } from "./MailViewParts";
import { ReaderNavContext, type ReaderNav } from "./readerNav";
import type { MessageGroupView, MessageRowView } from "./types";
import { Button } from "@/components/ui/button";
import { SINGLE_PANE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
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
  "hasMore" | "loadingMore" | "onLoadMore" | "readOnly" | "rowAction" | "queueLabel" | "empty"
> {
  /** URL of the list; an open conversation lives at `${basePath}/<thread>`. */
  basePath: string;
  title: string;
  meta?: ReactNode;
  actions?: ReactNode;
  /** Extra header content under the title row (filters, tabs). */
  toolbar?: ReactNode;
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
      threadIds: () => [
        ...new Set(groups.flatMap((group) => group.rows.map((row) => row.thread_id))),
      ],
      open,
      close,
      queueLabel: listProps.queueLabel,
    }),
    [close, groups, listProps.queueLabel, open],
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
            <div className="flex h-11 items-center gap-3 px-4">
              <h1 className="truncate text-[15px] font-semibold tracking-tight">{title}</h1>
              {meta ? (
                <span className="truncate font-mono text-2xs text-muted-foreground tabular-nums">
                  {meta}
                </span>
              ) : null}
              <span className="ml-auto flex items-center gap-0.5">{actions}</span>
            </div>
            {toolbar ? <div className="px-4 pb-2.5">{toolbar}</div> : null}
          </header>
          {status.isLoading ? (
            <ListSkeleton />
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
              groups={groups}
              scopeKey={scopeKey}
              label={title}
              activeThreadId={threadId}
              previewOnFocus={threadOpen && !hideList}
              onOpenRow={onOpenRow}
              onCloseThread={threadOpen ? close : undefined}
              empty={empty}
            />
          )}
        </section>
        {threadOpen ? <Outlet /> : null}
      </div>
    </ReaderNavContext.Provider>
  );
}
