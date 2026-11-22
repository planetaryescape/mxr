import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import { useCallback, useEffect, useRef, useState } from "react";

import {
  assistantKeys,
  cancelTurn,
  decide,
  fetchAvailability,
  fetchConversation,
  fetchConversations,
  fetchModels,
  sendMessage,
  setAutoApprove,
  setModel,
  type AssistantConversation,
  type AssistantDecision,
  type AssistantProgress,
  type AssistantViewContext,
} from "./api";
import { useAssistantUi } from "./assistantStore";
import type { DaemonEvent } from "@/api/events";
import { changedIds } from "./wire";
import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { useDaemonEvents } from "@/hooks/useDaemonEvents";
import { LANDING_MS, useLanding } from "@/state/landingStore";
import { useOpenThread } from "@/state/openThreadStore";
import { useSelection } from "@/state/selectionStore";

export function useAvailability() {
  return useQuery({
    queryKey: assistantKeys.availability,
    queryFn: fetchAvailability,
    staleTime: 30_000,
  });
}

export function useModels() {
  return useQuery({
    queryKey: assistantKeys.models,
    queryFn: fetchModels,
    staleTime: 10_000,
    retry: false,
  });
}

export function usePickModel() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: setModel,
    onSuccess: (models) => {
      queryClient.setQueryData(assistantKeys.models, models);
      void queryClient.invalidateQueries({ queryKey: assistantKeys.availability });
      void queryClient.invalidateQueries({ queryKey: ["llm-config"] });
      void queryClient.invalidateQueries({ queryKey: ["llm-status"] });
    },
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

export function useLiveTurn(conversationId: string | null): {
  progress: AssistantProgress | null;
  startedAt: number | null;
  writing: string | null;
} {
  const queryClient = useQueryClient();
  const [live, setLive] = useState<{
    id: string | null;
    progress: AssistantProgress | null;
    startedAt: number | null;
    writing: string | null;
  }>({ id: null, progress: null, startedAt: null, writing: null });

  const onEvent = useCallback(
    (event: DaemonEvent) => {
      if (event.type !== "AssistantUpdated") return;
      const update = event as Extract<DaemonEvent, { type: "AssistantUpdated" }>;
      if (update.progress?.kind !== "writing") {
        void queryClient.invalidateQueries({
          queryKey: assistantKeys.conversation(update.conversation_id),
        });
      }
      if (update.status !== "running") {
        void queryClient.invalidateQueries({ queryKey: assistantKeys.list });
      }
      if (update.conversation_id !== conversationId) return;
      setLive((current) => {
        if (update.status !== "running") {
          return {
            id: conversationId,
            progress: null,
            startedAt: null,
            writing: null,
          };
        }
        const same = current.id === conversationId;
        const startedAt = same && current.startedAt !== null ? current.startedAt : Date.now();
        const progress = update.progress ?? null;
        if (progress?.kind === "writing") {
          return {
            id: conversationId,
            progress: same ? current.progress : null,
            startedAt,
            writing: progress.text,
          };
        }
        return {
          id: conversationId,
          progress: progress ?? (same ? current.progress : null),
          startedAt,
          writing: progress === null && same ? current.writing : null,
        };
      });
    },
    [conversationId, queryClient],
  );
  useDaemonEvents(onEvent);

  return live.id === conversationId
    ? {
        progress: live.progress,
        startedAt: live.startedAt,
        writing: live.writing,
      }
    : { progress: null, startedAt: null, writing: null };
}

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
  const trust = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) => setAutoApprove(id, enabled),
    onSuccess: settle,
  });
  return { send, answer, stop, trust };
}

export function useLandChanges(conversation: AssistantConversation | undefined): void {
  const seen = useRef<{ id: string | null; calls: Set<string> }>({ id: null, calls: new Set() });
  useEffect(() => {
    if (!conversation) return;
    const applied = conversation.items.filter(
      (item): item is Extract<typeof item, { kind: "tool_call" }> =>
        item.kind === "tool_call" && (item.state === "approved" || item.state === "auto_approved"),
    );
    if (seen.current.id !== conversation.id) {
      seen.current = { id: conversation.id, calls: new Set(applied.map((item) => item.call_id)) };
      return;
    }
    const fresh = applied.filter((item) => !seen.current.calls.has(item.call_id));
    if (fresh.length === 0) return;
    for (const item of fresh) seen.current.calls.add(item.call_id);
    useLanding.getState().land(fresh.flatMap(changedIds));
    window.setTimeout(() => void invalidateMailQueries(), LANDING_MS);
  }, [conversation]);
}
