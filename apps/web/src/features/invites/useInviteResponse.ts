/*
 * React view of the module-level RSVP hold (inviteResponse.ts), for the
 * thread's invite card and the invites page.
 */

import { inviteReplyIntent, useComposeUi } from "@/features/compose/composeUiStore";

import {
  cancelInviteResponse,
  respondToInvite,
  useInviteRsvp,
  type InviteAction,
} from "./inviteResponse";

export type { InviteAction };

export function useInviteResponse({ messageId }: { messageId: string }) {
  const pendingAction = useInviteRsvp((state) => state.pending[messageId] ?? null);
  const isSubmitting = useInviteRsvp((state) => Boolean(state.sending[messageId]));
  return {
    begin: (action: InviteAction) => respondToInvite(messageId, action),
    cancel: () => cancelInviteResponse(messageId),
    pendingAction,
    isPending: pendingAction !== null,
    isSubmitting,
  };
}

/** "Accept with comment": open the composer on a reply that carries the RSVP. */
export function openInviteComment(
  messageId: string,
  action: InviteAction,
  surface: "inline" | "overlay" = "overlay",
): void {
  useComposeUi.getState().openCompose(inviteReplyIntent(messageId, action), surface);
}
