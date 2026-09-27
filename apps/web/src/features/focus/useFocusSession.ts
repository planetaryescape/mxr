/*
 * Focus mode's state: the queue (see focusQueue), which conversation's
 * reply is open, and what happens when a reply is sent, undone or fails.
 * A reply counts as done when its undo window opens, so the next one
 * comes up at once; undo (or a failed send) puts it back in front.
 */

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useReducer, useRef } from "react";

import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { onSendEvent } from "@/features/compose/session/sendEvents";
import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { fetchThread } from "@/features/mailbox/api";
import { fetchOwedReplies } from "@/features/owed/api";
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

export function useFocusSession() {
  const account = useUiPrefs((s) => s.accountScope);
  // Owed replies can be slow on a big mailbox; a window refocus shouldn't
  // ask again while you work through the queue. Sends refresh it anyway.
  const owed = useQuery({
    queryKey: ["owed", account],
    queryFn: () => fetchOwedReplies(account),
    staleTime: 60_000,
  });
  const replyLater = useQuery({ queryKey: ["reply-queue"], queryFn: fetchReplyQueue });
  const [session, dispatch] = useReducer(focusReducer, emptyFocusSession);

  // Start with whichever source answers first and add the other when it
  // does, so a slow or failed source never holds up the rest.
  const anySettled = !owed.isPending || !replyLater.isPending;
  const gathering = owed.isPending || replyLater.isPending;
  useEffect(() => {
    if (!anySettled) return;
    dispatch({
      kind: "sync",
      items: buildFocusQueue(owed.data?.rows ?? [], replyLater.data?.messages ?? [], account),
    });
  }, [anySettled, owed.data, replyLater.data, account]);

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
    error: owed.error ?? replyLater.error ?? null,
    skip: () => dispatch({ kind: "skip" }),
    markHandled: (threadId: string) => dispatch({ kind: "handled", threadId }),
    reopenReply: () => {
      if (current) useComposeUi.getState().openCompose(focusReplyIntent(current), "inline");
    },
  };
}
