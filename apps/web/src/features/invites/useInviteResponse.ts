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

export function openInviteComment(
  messageId: string,
  action: InviteAction,
  surface: "inline" | "overlay" = "overlay",
): void {
  useComposeUi.getState().openCompose(inviteReplyIntent(messageId, action), surface);
}
