/*
 * Focus mode's state: the queue (see focusQueue), which conversation's
 * reply is open, and what happens when a reply is sent, undone or fails.
 * A reply counts as done when its undo window opens, so the next one
 * comes up at once; undo (or a failed send) puts it back in front.
 */

import { useQueries, useQuery, useQueryClient, type UseQueryResult } from "@tanstack/react-query";
import { useEffect, useReducer, useRef } from "react";

import { fetchAccounts } from "@/features/accounts/api";
import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { onSendEvent } from "@/features/compose/session/sendEvents";
import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { fetchThread } from "@/features/mailbox/api";
import { fetchOwedReplies, type OwedReplyRow } from "@/features/owed/api";
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

/**
 * One sitting's worth of owed replies per account. The daemon has no
 * paging, so a full page means there may be more: the finish says so and
 * offers to continue rather than claiming that's everyone.
 */
const OWED_PER_ACCOUNT = 500;

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
  return { ids, accounts: scope ? null : accounts };
}

type OwedResult = UseQueryResult<{ rows: OwedReplyRow[] }>;

/**
 * Module scope, so TanStack keeps the combined result (and its `rows`
 * array) unchanged until a query's data does. Merged most overdue first.
 */
function combineOwed(results: OwedResult[]) {
  const pages = results.map((result) => result.data?.rows ?? []);
  return {
    rows:
      pages.length === 1
        ? (pages[0] ?? [])
        : pages.flat().toSorted((a, b) => b.overdue_score - a.overdue_score),
    pending: results.some((result) => result.isPending),
    error: results.find((result) => result.error)?.error ?? null,
    capped: pages.some((rows) => rows.length >= OWED_PER_ACCOUNT),
    refetch: () => Promise.all(results.map((result) => result.refetch())),
  };
}

/**
 * Owed replies for the account scope: that account, or every enabled one
 * under "All accounts" (the endpoint alone would answer for the default
 * account only).
 */
function useOwedAcrossScope(scope: string | null) {
  const { ids, accounts } = useScopeAccountIds(scope);
  const owed = useQueries({
    queries: ids.map((accountId) => ({
      queryKey: ["owed", accountId, { limit: OWED_PER_ACCOUNT }],
      queryFn: () => fetchOwedReplies(accountId, OWED_PER_ACCOUNT),
      // Owed replies can be slow on a big mailbox; a window refocus
      // shouldn't ask again mid-sitting. Sends refresh them anyway.
      staleTime: 60_000,
    })),
    combine: combineOwed,
  });
  return {
    ...owed,
    pending: Boolean(accounts?.isPending) || owed.pending,
    error: accounts?.error ?? owed.error,
    refetch: () => {
      void accounts?.refetch();
      return owed.refetch();
    },
  };
}

export function useFocusSession(lane?: "owed") {
  const account = useUiPrefs((s) => s.accountScope);
  const owed = useOwedAcrossScope(account);
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
    /** A full page of owed replies came back: there may be more. */
    capped: owed.capped,
    /** Conversations skipped when nothing else was left. */
    deferred: session.deferred.length,
    /** Ask again. Owed has no paging: after a capped batch, the ones you
     * replied to have dropped out, so asking again brings the next ones. */
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
