/*
 * The assistant's data: availability, conversations, and the live state
 * of a running turn. The daemon runs turns in the background and says
 * what changed with `AssistantUpdated` events; the conversation is
 * refetched on each, so the transcript is always the daemon's own.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import { useCallback, useState } from "react";

import {
  assistantKeys,
  cancelTurn,
  decide,
  fetchAvailability,
  fetchConversation,
  fetchConversations,
  sendMessage,
  type AssistantConversation,
  type AssistantDecision,
  type AssistantProgress,
  type AssistantViewContext,
} from "./api";
import { useAssistantUi } from "./assistantStore";
import type { DaemonEvent } from "@/api/events";
import { useDaemonEvents } from "@/hooks/useDaemonEvents";
import { useOpenThread } from "@/state/openThreadStore";
import { useSelection } from "@/state/selectionStore";

export function useAvailability() {
  return useQuery({
    queryKey: assistantKeys.availability,
    queryFn: fetchAvailability,
    staleTime: 30_000,
  });
}

export function useConversations(enabled: boolean) {
  return useQuery({
    queryKey: assistantKeys.list,
    queryFn: fetchConversations,
    enabled,
    staleTime: 10_000,
  });
}

export function useConversation(id: string | null) {
  return useQuery({
    queryKey: assistantKeys.conversation(id ?? ""),
    queryFn: () => fetchConversation(id ?? ""),
    enabled: id !== null,
    retry: false,
  });
}

/**
 * Keeps the open conversation in step with the daemon and returns what the
 * running turn is doing now, with when it started.
 */
export function useLiveTurn(conversationId: string | null): {
  progress: AssistantProgress | null;
  startedAt: number | null;
} {
  const queryClient = useQueryClient();
  const [live, setLive] = useState<{
    id: string | null;
    progress: AssistantProgress | null;
    startedAt: number | null;
  }>({ id: null, progress: null, startedAt: null });

  const onEvent = useCallback(
    (event: DaemonEvent) => {
      if (event.type !== "AssistantUpdated") return;
      const update = event as Extract<DaemonEvent, { type: "AssistantUpdated" }>;
      void queryClient.invalidateQueries({
        queryKey: assistantKeys.conversation(update.conversation_id),
      });
      if (update.status !== "running") {
        void queryClient.invalidateQueries({ queryKey: assistantKeys.list });
      }
      if (update.conversation_id !== conversationId) return;
      setLive((current) => {
        if (update.status !== "running") return { id: conversationId, progress: null, startedAt: null };
        const startedAt =
          current.id === conversationId && current.startedAt !== null ? current.startedAt : Date.now();
        return {
          id: conversationId,
          progress: update.progress ?? (current.id === conversationId ? current.progress : null),
          startedAt,
        };
      });
    },
    [conversationId, queryClient],
  );
  useDaemonEvents(onEvent);

  return live.id === conversationId
    ? { progress: live.progress, startedAt: live.startedAt }
    : { progress: null, startedAt: null };
}

/** What the user has open, so "this thread" and "these" mean something. */
export function useViewContext(): () => AssistantViewContext {
  const path = useRouterState({ select: (state) => state.location.pathname });
  const search = useRouterState({ select: (state) => state.location.search });
  return useCallback(() => {
    const threadId = useOpenThread.getState().threadId;
    const selected = [...useSelection.getState().ids].slice(0, 200);
    const view = path.split("/").find(Boolean) ?? "now";
    const query = (search as Record<string, unknown>).q;
    return {
      view,
      open_thread_id: threadId ?? undefined,
      selected_message_ids: selected,
      search_query: typeof query === "string" && query.trim() ? query : undefined,
    };
  }, [path, search]);
}

export function useAssistantActions() {
  const queryClient = useQueryClient();
  const showConversation = useAssistantUi((s) => s.showConversation);
  const settle = useCallback(
    (conversation: AssistantConversation) => {
      queryClient.setQueryData(assistantKeys.conversation(conversation.id), conversation);
      void queryClient.invalidateQueries({ queryKey: assistantKeys.list });
    },
    [queryClient],
  );
  const send = useMutation({
    mutationFn: sendMessage,
    onSuccess: (conversation) => {
      settle(conversation);
      showConversation(conversation.id);
    },
  });
  const answer = useMutation({
    mutationFn: ({ id, decisions }: { id: string; decisions: AssistantDecision[] }) =>
      decide(id, decisions),
    onSuccess: settle,
  });
  const stop = useMutation({ mutationFn: cancelTurn, onSuccess: settle });
  return { send, answer, stop };
}
