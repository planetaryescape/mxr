/*
 * Moving mail between modes (D119): `X` moves one email, `K` sends
 * everything from its sender. The daemon stores each as a correction and
 * tells every client (`ModesChanged`); this file runs the request, shows
 * the toast with Undo and "Always for this sender?", and keeps `u` on the
 * newest move like any other mail change.
 */

import { toast } from "sonner";
import { create } from "zustand";

import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { claimUndo } from "@/features/mail-actions/mailUndo";
import { toneFor } from "@/features/mail-actions/verbFeedback";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { useUndo } from "@/state/undoStore";

import { moveMessage, undoMove, type MoveRequest } from "./api";
import type { MoveOutcome } from "./types";
import { hintSeen, markHintSeen } from "./useTrustHint";

/** What a picker moves: one email, named for the dialog's title. */
export interface MoveSubject {
  messageId: string;
  /** "Q4 plan" or the sender, for the title. */
  label: string;
  /** Who sent it, for the sender picker's title. */
  senderLabel?: string;
}

/** Open the destination picker: this email (`X`) or its sender (`K`). */
export function openMovePicker(subject: MoveSubject, sender = false): void {
  if (refuseWhileDaemonDown(sender ? "move the sender" : "move it")) return;
  openMailDialog({ kind: "move-to-mode", subject, sender });
}

interface SenderAskState {
  /** The move whose toast asks "Always for this sender?", while it shows. */
  pending: MoveOutcome | null;
  set: (pending: MoveOutcome | null) => void;
}

export const useSenderAsk = create<SenderAskState>((set) => ({
  pending: null,
  set: (pending) => set({ pending }),
}));

const toastId = (outcome: MoveOutcome) => `move-${outcome.message_id}`;

/**
 * `K` while a move's toast asks: answer it for that email's sender. False
 * when no toast is asking, so `K` opens the sender picker instead.
 */
export function answerPendingSenderAsk(): boolean {
  const pending = useSenderAsk.getState().pending;
  if (!pending) return false;
  void alwaysForSender(pending);
  return true;
}

/** Send every email from the moved email's sender to the same mode. */
export async function alwaysForSender(outcome: MoveOutcome): Promise<MoveOutcome | null> {
  useSenderAsk.getState().set(null);
  toast.dismiss(toastId(outcome));
  return performMove({ messageId: outcome.message_id, mode: outcome.to, sender: true });
}

/**
 * Run a move and announce it. Holds the undo slot from the key press, so
 * `u` pressed before the daemon answers still undoes this move.
 */
export async function performMove(
  request: MoveRequest,
  options: { askInToast?: boolean } = {},
): Promise<MoveOutcome | null> {
  if (refuseWhileDaemonDown("move it")) return null;
  const claim = claimUndo();
  try {
    const outcome = await moveMessage(request);
    // A Not-sure answer asks about the sender on its own row instead.
    const shown = options.askInToast === false ? { ...outcome, ask_sender: null } : outcome;
    claim.settle(announceMove(shown, claim.run));
    return outcome;
  } catch (caught) {
    claim.settle(null);
    toast.error("Couldn't move it", {
      description: caught instanceof Error ? caught.message : String(caught),
    });
    return null;
  } finally {
    await invalidateMailQueries().catch(() => undefined);
  }
}

/** Put a move back; the daemon says where it went. */
export async function reverseMove(correctionId: number): Promise<boolean> {
  try {
    const copy = await undoMove(correctionId);
    toast.success(copy);
    return true;
  } catch (caught) {
    toast.error("Undo failed", {
      description: caught instanceof Error ? caught.message : String(caught),
    });
    return false;
  } finally {
    await invalidateMailQueries().catch(() => undefined);
  }
}

/** The toast for a move; returns its undo for the claimed slot. */
export function announceMove(
  outcome: MoveOutcome,
  claim: () => Promise<boolean>,
): (() => Promise<boolean>) | null {
  const id = toastId(outcome);
  const correctionId = outcome.correction_id;
  const newest = useUndo.getState().lastUndo === claim;
  const show = toast[toneFor("mode-move")];
  if (correctionId === null || correctionId === undefined) {
    if (newest) useUndo.getState().recordNoUndo();
    show(outcome.copy, { id });
    return null;
  }
  const undo = async () => {
    useUndo.getState().retireUndo(undo);
    if (useSenderAsk.getState().pending?.message_id === outcome.message_id) {
      useSenderAsk.getState().set(null);
    }
    toast.dismiss(id);
    return reverseMove(correctionId);
  };
  if (newest) useUndo.getState().recordUndo(undo);

  const ask = outcome.ask_sender ?? null;
  const hint = outcome.hint && !hintSeen("now.move") ? outcome.hint : null;
  if (hint) markHintSeen("now.move");
  if (ask) useSenderAsk.getState().set(outcome);
  const clearAsk = () => {
    if (useSenderAsk.getState().pending === outcome) useSenderAsk.getState().set(null);
  };
  show(ask ? `${outcome.copy} ${ask}` : outcome.copy, {
    id,
    duration: ask ? 15_000 : 60_000,
    description: hint ?? "Press u to undo",
    action: { label: "Undo", onClick: () => void undo() },
    cancel: ask
      ? { label: "Always for this sender", onClick: () => void alwaysForSender(outcome) }
      : undefined,
    onDismiss: clearAsk,
    onAutoClose: clearAsk,
  });
  return undo;
}
