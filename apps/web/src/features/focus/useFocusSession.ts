/*
 * Focus mode's state: the queue (see focusQueue), which conversation's
 * reply is open, and what happens when a reply is sent, undone or fails.
 * A reply counts as done when its undo window opens, so the next one
 * comes up at once; undo (or a failed send) puts it back in front.
 */

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useReducer, useRef } from "react";

import { fetchAccounts } from "@/features/accounts/api";
import { DESK_FULL_LANE_LIMIT, deskKey, fetchDesk, type DeskRow } from "@/features/desk/api";
import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { onSendEvent } from "@/features/compose/session/sendEvents";
import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { fetchThread } from "@/features/mailbox/api";
import { fetchReplyQueue } from "@/features/reply-queue/api";
import { threadContextQuery } from "@/features/thread/context/api";
import { useUiPrefs } from "@/state/uiPrefsStore";

import {
  buildFocusQueue,
  emptyFocusSession,
  focusProgress,
  focusReducer,
  type FocusItem,
} from "./focusQueue";

export function focusReplyIntent(item: FocusItem) {
  return replyIntent(item.messageId, "single");
}

/** Account ids a scope covers: that account, or every enabled one. */
export function useScopeAccountIds(scope: string | null) {
  const accounts = useQuery({
    queryKey: ["accounts"],
    queryFn: fetchAccounts,
    staleTime: 60_000,
    enabled: !scope,
  });
  const ids = scope
    ? [scope]
    : (accounts.data?.accounts ?? []).filter((a) => a.enabled).map((a) => a.account_id);
  return { ids };
}

/**
 * Everyone you owe, as the desk's You owe lane decides it: people only,
 * in the inbox, people you have written to, with the screener, snoozes and
 * the account scope applied by the daemon. The whole lane, the way the
 * desk's "Show all" asks for it.
 */
function useOwedLane(scope: string | null) {
  const desk = useQuery({
    queryKey: deskKey(scope, DESK_FULL_LANE_LIMIT),
    queryFn: () => fetchDesk(scope, DESK_FULL_LANE_LIMIT),
    staleTime: 15_000,
  });
  const lane = desk.data?.owed;
  return {
    rows: lane?.rows ?? NO_ROWS,
    /** Owed conversations beyond what the lane returned. */
    more: lane ? Math.max(0, lane.total - lane.rows.length) : 0,
    pending: desk.isPending,
    error: desk.error,
    refetch: () => desk.refetch(),
  };
}

const NO_ROWS: readonly DeskRow[] = [];

export function useFocusSession(lane?: "owed") {
  const account = useUiPrefs((s) => s.accountScope);
  const owed = useOwedLane(account);
  // The desk's You owe lane is owed replies only.
  const withReplyLater = lane !== "owed";
  const replyLater = useQuery({
    queryKey: ["reply-queue"],
    queryFn: fetchReplyQueue,
    enabled: withReplyLater,
  });
  const replyLaterPending = withReplyLater && replyLater.isPending;
  const [session, dispatch] = useReducer(focusReducer, emptyFocusSession);

  // Start with whichever source answers first and add the other when it
  // does, so a slow or failed source never holds up the rest.
  const anySettled = !owed.pending || !replyLaterPending;
  const gathering = owed.pending || replyLaterPending;
  useEffect(() => {
    if (!anySettled) return;
    dispatch({
      kind: "sync",
      items: buildFocusQueue(
        owed.rows,
        withReplyLater ? (replyLater.data?.messages ?? []) : [],
        account,
      ),
    });
  }, [anySettled, owed.rows, replyLater.data, account, withReplyLater]);

  const currentId = session.queue[0];
  const current = currentId ? session.items[currentId] : undefined;

  // Which conversation each reply intent belongs to, for send events that
  // arrive after its composer has closed.
  const intentThreads = useRef(new Map<string, string>());
  const sendsInFlight = useRef(0);
  useEffect(
    () =>
      onSendEvent((event) => {
        const threadId = intentThreads.current.get(event.intentKey);
        if (!threadId) return;
        switch (event.kind) {
          case "queued":
            sendsInFlight.current += 1;
            dispatch({ kind: "handled", threadId });
            break;
          case "cancelled":
          case "failed":
            sendsInFlight.current = Math.max(0, sendsInFlight.current - 1);
            dispatch({ kind: "restore", threadId });
            break;
          case "sent":
            sendsInFlight.current = Math.max(0, sendsInFlight.current - 1);
            void invalidateMailQueries();
            break;
        }
      }),
    [],
  );

  // A reply in its undo window lives only in this tab: warn before closing.
  useEffect(() => {
    const warn = (event: BeforeUnloadEvent) => {
      if (sendsInFlight.current > 0) event.preventDefault();
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, []);

  // The reply for the conversation on screen, inline in focus mode's slot.
  useEffect(() => {
    if (!current) return;
    const intent = focusReplyIntent(current);
    intentThreads.current.set(intent.key, current.threadId);
    if (useComposeUi.getState().intent?.key === intent.key) return;
    useComposeUi.getState().openCompose(intent, "inline");
  }, [current]);

  // Leaving focus mode closes the reply it opened (the draft is saved).
  useEffect(
    () => () => {
      const open = useComposeUi.getState().intent;
      if (open && intentThreads.current.has(open.key)) useComposeUi.getState().closeCompose();
    },
    [],
  );

  // The next conversation is usually a keypress away: have it ready.
  const queryClient = useQueryClient();
  const nextId = session.queue[1];
  useEffect(() => {
    if (!nextId) return;
    void queryClient.prefetchQuery({
      queryKey: ["thread", nextId],
      queryFn: () => fetchThread(nextId),
    });
    void queryClient.prefetchQuery(threadContextQuery(nextId));
  }, [nextId, queryClient]);

  return {
    current,
    progress: focusProgress(session),
    // Nothing to show yet, or nothing so far while a source is still out:
    // never call it finished before everyone is known.
    loading: !session.loaded || (session.queue.length === 0 && gathering),
    gathering,
    error: owed.error ?? (withReplyLater ? replyLater.error : null) ?? null,
    /** Owed conversations the lane didn't return (it is capped). */
    more: owed.more,
    /** Conversations skipped when nothing else was left. */
    deferred: session.deferred.length,
    /** Ask again; after a capped lane, the ones you replied to have
     * dropped out, so asking again brings the rest. */
    refetch: () => {
      void owed.refetch();
      if (withReplyLater) void replyLater.refetch();
    },
    revisit: () => dispatch({ kind: "revisit" }),
    skip: () => dispatch({ kind: "skip" }),
    markHandled: (threadId: string) => dispatch({ kind: "handled", threadId }),
    reopenReply: () => {
      if (current) useComposeUi.getState().openCompose(focusReplyIntent(current), "inline");
    },
  };
}
