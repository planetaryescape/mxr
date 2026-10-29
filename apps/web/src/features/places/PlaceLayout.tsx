import { Outlet, useNavigate, useParams } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useRef, type ReactNode } from "react";

import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { LIST_PANE_SIZE } from "@/features/mailbox/listPane";
import { FramePanesContext, READER_TOOLBAR_HEIGHT } from "@/components/PageFrame";
import { ReaderNavContext, type ReaderNav } from "@/features/mailbox/readerNav";
import { conversationKey, selectedKey, type ThreadRef } from "@/features/mailbox/rowKey";
import {
  useAdvanceOnRemoval,
  useRouteAccount,
  useRouteStillShows,
} from "@/hooks/useAdvanceOnRemoval";
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
  threads,
  wideReader = false,
  keepSearch = NO_KEYS,
  children,
}: {
  basePath: "/reading" | "/updates" | "/todo" | "/archive";
  label: string;
  /** Conversations in the order the place shows them, each with its account when it has one. */
  threads: () => ThreadRef[];
  wideReader?: boolean;
  keepSearch?: readonly string[];
  children: ReactNode;
}) {
  const navigate = useNavigate();
  const params = useParams({ strict: false }) as { threadId?: string };
  const routeAccount = useRouteAccount();
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

  const refsRef = useRef<ThreadRef[]>([]);
  const open = useCallback(
    (threadId: string, options?: { focusReader?: boolean; accountId?: string | null }) => {
      setActivePane(options?.focusReader === false ? "mailbox" : "reader");
      // A caller that names no account gets the first listed row's with that id.
      const accountId =
        options?.accountId ??
        refsRef.current.find((ref) => ref.thread_id === threadId)?.account_id ??
        null;
      void navigate({
        to: `${basePath}/${encodeURIComponent(threadId)}`,
        search: (prev: Record<string, unknown>) => ({
          ...pickSearch(prev, keepSearch),
          ...(accountId ? { account: accountId } : {}),
        }),
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
  // A conversation is its thread within its account: two accounts can hold one thread id.
  const refs = useMemo(() => threads(), [threads]);
  refsRef.current = refs;
  const keys = useMemo(
    () => [...new Set(refs.map((ref) => conversationKey(ref.thread_id, ref.account_id)))],
    [refs],
  );
  const keysRef = useRef(keys);
  keysRef.current = keys;
  // Rows that left the place are gone from `refs`; their refs are still needed to read their key back.
  const knownRefs = useRef(new Map<string, ThreadRef>());
  for (const ref of refs)
    knownRefs.current.set(conversationKey(ref.thread_id, ref.account_id), ref);
  const refOf = (key: string): ThreadRef => knownRefs.current.get(key) ?? { thread_id: key };
  const isListed = useCallback(
    (id: string, accountId?: string | null) =>
      keysRef.current.includes(conversationKey(id, accountId)),
    [],
  );
  const stillShows = useRouteStillShows();
  const advance = useAdvanceOnRemoval({
    ids: keys,
    selectedId: params.threadId ? selectedKey(refs, params.threadId, routeAccount) : null,
    onAdvance: (next, from) => {
      if (from !== null) {
        const gone = refOf(from);
        if (!stillShows(gone.thread_id, gone.account_id)) return;
      }
      const to = refOf(next);
      open(to.thread_id, {
        focusReader: useMailboxPane.getState().activePane === "reader",
        accountId: to.account_id,
      });
    },
    onEmpty: (removed) => {
      const gone = refOf(removed);
      if (stillShows(gone.thread_id, gone.account_id)) close();
    },
  });
  const expectRemoval = useCallback<NonNullable<ReaderNav["expectRemoval"]>>(
    (id, direction, accountId) => advance(conversationKey(id, accountId), direction),
    [advance],
  );
  const nav = useMemo<ReaderNav>(
    () => ({ threads, open, close, expectRemoval, isListed }),
    [close, expectRemoval, isListed, open, threads],
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
              data-mode-pane=""
              tabIndex={-1}
              className="mode-page flex min-h-0 min-w-0 flex-1 flex-col bg-background outline-none"
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
            {threadOpen ? (
              <div className="reader-arrive flex min-h-0 min-w-0 flex-1">
                <Outlet />
              </div>
            ) : null}
          </ResizablePanel>
        </ResizablePanelGroup>
      </FramePanesContext.Provider>
    </ReaderNavContext.Provider>
  );
}
