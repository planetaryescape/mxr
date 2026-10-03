import { Outlet, useNavigate, useParams } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, type ReactNode } from "react";

import { ReaderNavContext, type ReaderNav } from "@/features/mailbox/readerNav";
import { SINGLE_PANE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

/**
 * A place's list with the reader beside it, as the mail lists have it: an
 * open conversation lives at `${basePath}/<thread>`, Esc closes it and n/N
 * step through the conversations the place shows. `wideReader` gives the
 * reader the whole width (Reading's issues are already open in the feed).
 */
export function PlaceLayout({
  basePath,
  label,
  threadIds,
  wideReader = false,
  children,
}: {
  basePath: "/reading" | "/paper-trail" | "/todo";
  label: string;
  /** Conversations in the order the place shows them. */
  threadIds: () => string[];
  wideReader?: boolean;
  children: ReactNode;
}) {
  const navigate = useNavigate();
  const params = useParams({ strict: false }) as { threadId?: string };
  const threadOpen = Boolean(params.threadId);
  const singlePane = useMediaQuery(SINGLE_PANE_QUERY);
  const readerLayout = useUiPrefs((s) => s.readerLayout);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const hideList = threadOpen && (singlePane || wideReader || readerLayout === "full");

  useEffect(() => {
    if (!threadOpen && useMailboxPane.getState().activePane === "reader") setActivePane("mailbox");
  }, [setActivePane, threadOpen]);

  const open = useCallback(
    (threadId: string, options?: { focusReader?: boolean }) => {
      setActivePane(options?.focusReader === false ? "mailbox" : "reader");
      void navigate({ to: `${basePath}/${encodeURIComponent(threadId)}` });
    },
    [basePath, navigate, setActivePane],
  );
  const close = useCallback(() => {
    setActivePane("mailbox");
    void navigate({ to: basePath });
  }, [basePath, navigate, setActivePane]);
  const nav = useMemo<ReaderNav>(() => ({ threadIds, open, close }), [close, open, threadIds]);

  return (
    <ReaderNavContext.Provider value={nav}>
      <div className="flex min-h-0 min-w-0 flex-1">
        <section
          aria-label={label}
          className={cn(
            "@container flex min-h-0 min-w-0 flex-col bg-background",
            threadOpen ? "w-[clamp(340px,36%,480px)] shrink-0 border-r border-border" : "flex-1",
            hideList && "hidden",
          )}
        >
          {children}
        </section>
        {threadOpen ? <Outlet /> : null}
      </div>
    </ReaderNavContext.Provider>
  );
}

/** The place's header: title, a quiet meta line and trailing actions. */
export function PlaceHeader({
  title,
  meta,
  actions,
}: {
  title: string;
  meta?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="flex shrink-0 flex-wrap items-baseline gap-x-3 gap-y-1 border-b border-border px-5 pb-3 pt-4">
      <h1 className="text-[17px] font-semibold tracking-tight text-foreground">{title}</h1>
      {meta ? <p className="min-w-0 text-[12.5px] text-muted-foreground">{meta}</p> : null}
      {actions ? <div className="ml-auto flex items-center gap-2">{actions}</div> : null}
    </header>
  );
}
