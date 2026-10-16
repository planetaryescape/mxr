import { Outlet, useNavigate, useParams } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useRef, type ReactNode } from "react";

import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { LIST_PANE_SIZE } from "@/features/mailbox/listPane";
import { FramePanesContext, READER_TOOLBAR_HEIGHT } from "@/components/PageFrame";
import { ReaderNavContext, type ReaderNav } from "@/features/mailbox/readerNav";
import { useAdvanceOnRemoval, useRouteStillShows } from "@/hooks/useAdvanceOnRemoval";
import { SINGLE_PANE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { pickSearch } from "@/hooks/useSearchParam";
import { useSplitPane } from "@/hooks/useSplitPane";
import { ownsKeyboard } from "@/lib/keys/dispatcher";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

const NO_KEYS: readonly string[] = [];

/**
 * A place's list with the reader beside it, as the mail lists have it: an
 * open conversation lives at `${basePath}/<thread>`, Esc closes it and n/N
 * step through the conversations the place shows. `wideReader` gives the
 * reader the whole width (Reading's issues are already open in the feed).
 * `keepSearch` names the list's own search params (its view and cursor),
 * which stay in the URL while a conversation opens and closes; the
 * reader's own params don't carry over to the next one.
 */
export function PlaceLayout({
  basePath,
  label,
  threadIds,
  wideReader = false,
  keepSearch = NO_KEYS,
  children,
}: {
  basePath: "/reading" | "/updates" | "/todo" | "/archive";
  label: string;
  /** Conversations in the order the place shows them. */
  threadIds: () => string[];
  wideReader?: boolean;
  keepSearch?: readonly string[];
  children: ReactNode;
}) {
  const navigate = useNavigate();
  const params = useParams({ strict: false }) as { threadId?: string };
  const threadOpen = Boolean(params.threadId);
  const singlePane = useMediaQuery(SINGLE_PANE_QUERY);
  const readerLayout = useUiPrefs((s) => s.readerLayout);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const hideList = threadOpen && (singlePane || wideReader || readerLayout === "full");
  const split = threadOpen && !hideList;
  const panes = useMemo(() => ({ split, inset: READER_TOOLBAR_HEIGHT }), [split]);
  const pane = useSplitPane(`place${basePath.replace("/", ":")}`, LIST_PANE_SIZE, {
    active: split,
  });

  useEffect(() => {
    if (!threadOpen && useMailboxPane.getState().activePane === "reader") setActivePane("mailbox");
  }, [setActivePane, threadOpen]);

  const open = useCallback(
    (threadId: string, options?: { focusReader?: boolean }) => {
      setActivePane(options?.focusReader === false ? "mailbox" : "reader");
      void navigate({
        to: `${basePath}/${encodeURIComponent(threadId)}`,
        search: (prev: Record<string, unknown>) => pickSearch(prev, keepSearch),
      } as never);
    },
    [basePath, keepSearch, navigate, setActivePane],
  );
  const close = useCallback(() => {
    setActivePane("mailbox");
    void navigate({
      to: basePath,
      search: (prev: Record<string, unknown>) => pickSearch(prev, keepSearch),
    } as never);
  }, [basePath, keepSearch, navigate, setActivePane]);

  // Esc steps back from an open conversation. The reader's own scope handles
  // it while the reader has focus; opened from a link (or with the list
  // focused) only the list's scope is live and it has no Esc, so catch the
  // key the dispatcher left alone. The light is still on the row, which
  // `keepSearch` holds in the URL.
  useEffect(() => {
    if (!threadOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
      if (ownsKeyboard(event.target) || document.querySelector('[role="dialog"]')) return;
      event.preventDefault();
      close();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [close, threadOpen]);

  // The open conversation left the place (done from the list, sync): open
  // its neighbour, keeping the keys where they were.
  const ids = useMemo(() => threadIds(), [threadIds]);
  const idsRef = useRef(ids);
  idsRef.current = ids;
  const isListed = useCallback((id: string) => idsRef.current.includes(id), []);
  const stillShows = useRouteStillShows();
  const expectRemoval = useAdvanceOnRemoval({
    ids,
    selectedId: params.threadId ?? null,
    onAdvance: (next, from) => {
      if (from !== null && !stillShows(from)) return;
      open(next, { focusReader: useMailboxPane.getState().activePane === "reader" });
    },
    onEmpty: (removed) => {
      if (stillShows(removed)) close();
    },
  });
  const nav = useMemo<ReaderNav>(
    () => ({ threadIds, open, close, expectRemoval, isListed }),
    [close, expectRemoval, isListed, open, threadIds],
  );

  return (
    <ReaderNavContext.Provider value={nav}>
      <FramePanesContext.Provider value={panes}>
        <ResizablePanelGroup className="relative min-h-0 min-w-0 flex-1" {...pane.groupProps}>
          <ResizablePanel
            id="place-list"
            data-parked={hideList || undefined}
            {...pane.sidePanelProps}
            className="flex min-h-0 flex-col"
          >
            <section
              aria-label={label}
              className="mode-page flex min-h-0 min-w-0 flex-1 flex-col bg-background"
            >
              {children}
            </section>
          </ResizablePanel>
          {/* The handle and reader stay mounted, parked off screen while
            unused (app.css): registering either with the library on open
            reads layout mid-commit, which pushed opening a conversation
            past the speed gate. */}
          <ResizableHandle
            aria-label={`Resize ${label} list`}
            data-parked={!split || undefined}
            {...pane.handleProps}
          />
          <ResizablePanel
            id="place-reader"
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
