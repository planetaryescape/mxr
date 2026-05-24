import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";

import { apiFetch } from "@/api/client";

export type InviteAction = "accept" | "tentative" | "decline";

interface InviteResponseRequest {
  message_id: string;
  action: InviteAction;
}

interface InviteResponseResult {
  status: string;
  result?: unknown;
  preview?: unknown;
}

const UNDO_WINDOW_MS = 1000;

interface UseInviteResponseArgs {
  messageId: string;
  /// Thread to invalidate on success (the thread-view invite card). Omit
  /// when responding from a context without a thread, e.g. the invites list.
  threadId?: string;
  /// Extra React Query keys to invalidate on success. The invites list page
  /// passes `[["invites"]]` so its RSVP column refreshes.
  invalidateKeys?: readonly unknown[][];
}

/// Hold-and-send for a calendar invite RSVP. The 1s window lets the user
/// click "Undo" before any network call fires — no email goes out unless the
/// timer elapses. Mirrors the TUI's `pending_invite_send` semantics.
export function useInviteResponse({
  messageId,
  threadId,
  invalidateKeys,
}: UseInviteResponseArgs) {
  const qc = useQueryClient();
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [pendingAction, setPendingAction] = useState<InviteAction | null>(null);

  const mutation = useMutation({
    mutationKey: ["invite-reply", messageId],
    mutationFn: (body: InviteResponseRequest) =>
      apiFetch<InviteResponseResult>("/api/v1/mail/actions/invite/reply", {
        method: "POST",
        body,
      }),
    onSuccess: () => {
      if (threadId) {
        qc.invalidateQueries({ queryKey: ["thread", threadId] });
      }
      for (const key of invalidateKeys ?? []) {
        qc.invalidateQueries({ queryKey: key });
      }
    },
  });

  const cancel = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    setPendingAction(null);
  }, []);

  const begin = useCallback(
    (action: InviteAction) => {
      cancel();
      setPendingAction(action);
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        setPendingAction(null);
        mutation.mutate({ message_id: messageId, action });
      }, UNDO_WINDOW_MS);
    },
    [cancel, mutation, messageId],
  );

  useEffect(() => cancel, [cancel]);

  return {
    begin,
    cancel,
    pendingAction,
    isPending: pendingAction !== null,
    isSubmitting: mutation.isPending,
    error: mutation.error,
  };
}

/// Open a compose session for the "Reply with comment" path. Returns the
/// session-id JSON returned by the bridge so the caller can navigate to the
/// compose route.
export async function openInviteReplyComposeSession(
  messageId: string,
  action: InviteAction,
): Promise<unknown> {
  return apiFetch<unknown>("/api/v1/mail/compose/session", {
    method: "POST",
    body: {
      kind: "invite_reply",
      message_id: messageId,
      action,
    },
  });
}
