/*
 * Calendar RSVP with a short hold, like the TUI's pending invite send: no
 * email leaves until the window closes, and Undo inside the window cancels
 * it. The hold lives at module level so navigating away doesn't drop a
 * response the user already chose, and the toast switches to "Sending" the
 * moment the request starts so Undo can never claim to cancel a sent reply.
 */

import { toast } from "sonner";
import { create } from "zustand";

import { apiFetch } from "@/api/client";
import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { getActiveQueryClient } from "@/lib/queryClient";

export type InviteAction = "accept" | "tentative" | "decline";

export const RSVP_HOLD_MS = 1500;

const VERB: Record<InviteAction, { doing: string; done: string }> = {
  accept: { doing: "Accepting", done: "Accepted" },
  tentative: { doing: "Replying maybe", done: "Replied maybe" },
  decline: { doing: "Declining", done: "Declined" },
};

interface PendingRsvp {
  action: InviteAction;
  timer: ReturnType<typeof setTimeout>;
}

interface InviteRsvpState {
  /** messageId → action, while held or sending. */
  pending: Record<string, InviteAction>;
  sending: Record<string, boolean>;
}

export const useInviteRsvp = create<InviteRsvpState>(() => ({ pending: {}, sending: {} }));

const holds = new Map<string, PendingRsvp>();

function setPending(messageId: string, action: InviteAction | null) {
  useInviteRsvp.setState((state) => {
    const pending = { ...state.pending };
    if (action) pending[messageId] = action;
    else delete pending[messageId];
    return { pending };
  });
}

function setSending(messageId: string, sending: boolean) {
  useInviteRsvp.setState((state) => {
    const next = { ...state.sending };
    if (sending) next[messageId] = true;
    else delete next[messageId];
    return { sending: next };
  });
}

export function cancelInviteResponse(messageId: string): boolean {
  const hold = holds.get(messageId);
  if (!hold) return false;
  clearTimeout(hold.timer);
  holds.delete(messageId);
  setPending(messageId, null);
  toast.success("RSVP cancelled", { id: `rsvp-${messageId}`, duration: 2000 });
  return true;
}

export function respondToInvite(messageId: string, action: InviteAction): void {
  const existing = holds.get(messageId);
  if (existing) clearTimeout(existing.timer);
  setPending(messageId, action);
  const toastId = `rsvp-${messageId}`;
  toast(`${VERB[action].doing}…`, {
    id: toastId,
    duration: RSVP_HOLD_MS + 500,
    description: "The reply goes out in a moment",
    action: { label: "Undo", onClick: () => cancelInviteResponse(messageId) },
  });
  const timer = setTimeout(() => {
    holds.delete(messageId);
    void send(messageId, action, toastId);
  }, RSVP_HOLD_MS);
  holds.set(messageId, { action, timer });
}

async function send(messageId: string, action: InviteAction, toastId: string) {
  setSending(messageId, true);
  toast.loading(`${VERB[action].doing}…`, { id: toastId, description: "Sending reply" });
  try {
    await apiFetch<unknown>("/api/v1/mail/actions/invite/reply", {
      method: "POST",
      body: { message_id: messageId, action },
    });
    toast.success(`${VERB[action].done}`, { id: toastId, description: undefined });
    const qc = getActiveQueryClient();
    void qc?.invalidateQueries({ queryKey: ["invites"] });
    void invalidateMailQueries(qc);
  } catch (error) {
    toast.error("RSVP failed", {
      id: toastId,
      description: error instanceof Error ? error.message : String(error),
      action: { label: "Retry", onClick: () => respondToInvite(messageId, action) },
    });
  } finally {
    setSending(messageId, false);
    setPending(messageId, null);
  }
}
