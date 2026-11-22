import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useReducer, useRef } from "react";

import { fetchAccounts } from "@/features/accounts/api";
import { DESK_FULL_LANE_LIMIT, deskKey, fetchDesk, type DeskRow } from "@/features/desk/api";
import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import type { ComposeIntent } from "@/features/compose/session/composeDraft";
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

export function focusReplyIntent(item: FocusItem): ComposeIntent {
  return { ...replyIntent(item.messageId, "single"), quiet: true };
}

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

function useOwedLane(scope: string | null) {
  const desk = useQuery({
    queryKey: deskKey(scope, DESK_FULL_LANE_LIMIT),
    queryFn: () => fetchDesk(scope, DESK_FULL_LANE_LIMIT),
    staleTime: 15_000,
  });
  const lane = desk.data?.owed;
  return {
    rows: lane?.rows ?? NO_ROWS,
    more: lane ? Math.max(0, lane.total - lane.rows.length) : 0,
    others: desk.data?.people?.others ?? 0,
    pending: desk.isPending,
    error: desk.error,
    refetch: () => desk.refetch(),
  };
}

const NO_ROWS: readonly DeskRow[] = [];

export function useFocusSession(lane?: "owed", start?: string) {
  const account = useUiPrefs((s) => s.accountScope);
  const owed = useOwedLane(account);
  const withReplyLater = lane !== "owed";
  const replyLater = useQuery({
    queryKey: ["reply-queue"],
    queryFn: fetchReplyQueue,
    enabled: withReplyLater,
  });
  const replyLaterPending = withReplyLater && replyLater.isPending;
  const [session, dispatch] = useReducer(focusReducer, emptyFocusSession);

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

  const startWith = useRef(start);
  useEffect(() => {
    const wanted = startWith.current;
    if (!wanted || !session.loaded) return;
    if (session.queue.includes(wanted)) {
      startWith.current = undefined;
      dispatch({ kind: "front", threadId: wanted });
    } else if (!gathering) {
      startWith.current = undefined;
    }
  }, [gathering, session.loaded, session.queue]);

  const currentId = session.queue[0];
  const current = currentId ? session.items[currentId] : undefined;

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

  useEffect(() => {
    const warn = (event: BeforeUnloadEvent) => {
      if (sendsInFlight.current > 0) event.preventDefault();
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, []);

  useEffect(() => {
    if (!current) {
      const open = useComposeUi.getState().intent;
      if (open && intentThreads.current.has(open.key)) useComposeUi.getState().closeCompose();
      return;
    }
    const intent = focusReplyIntent(current);
    intentThreads.current.set(intent.key, current.threadId);
    if (useComposeUi.getState().intent?.key === intent.key) return;
    useComposeUi.getState().openCompose(intent, "inline");
  }, [current]);

  useEffect(
    () => () => {
      const open = useComposeUi.getState().intent;
      if (open && intentThreads.current.has(open.key)) useComposeUi.getState().closeCompose();
    },
    [],
  );

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
    loading: !session.loaded || (session.queue.length === 0 && gathering),
    gathering,
    error: owed.error ?? (withReplyLater ? replyLater.error : null) ?? null,
    more: owed.more,
    others: owed.others,
    deferred: session.deferred.length,
    refetch: () => {
      void owed.refetch();
      if (withReplyLater) void replyLater.refetch();
    },
    revisit: () => dispatch({ kind: "revisit" }),
    skip: () => dispatch({ kind: "skip" }),
    markHandled: (threadId: string) => dispatch({ kind: "handled", threadId }),
    restore: (threadId: string) => dispatch({ kind: "restore", threadId }),
    reopenReply: () => {
      if (current) useComposeUi.getState().openCompose(focusReplyIntent(current), "inline");
    },
  };
}
