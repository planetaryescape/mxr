import { createElement } from "react";
import { toast } from "sonner";
import { create } from "zustand";

import { UNDO_TOAST_DURATION_MS, UndoLabel } from "@/components/ui/sonner";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { invalidateMailQueries } from "@/features/mail-actions/mailQueryInvalidation";
import { claimUndo } from "@/features/mail-actions/mailUndo";
import { toneFor } from "@/features/mail-actions/verbFeedback";
import type { ScreenerQuestion } from "@/features/modes/membership";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { useUndo } from "@/state/undoStore";

import { moveMessage, undoMove, type MoveRequest } from "./api";
import type { ModeId, MoveOutcome } from "./types";
import { hintSeen, markHintSeen } from "./useTrustHint";

export interface MoveSubject {
  messageId: string;
  label: string;
  senderLabel?: string;
  notSure?: { mode: ModeId; answer: (mode: ModeId, sender: boolean) => void };
  here?: ModeId;
  newSender?: { question: ScreenerQuestion; label: string };
}

export function openMovePicker(subject: MoveSubject, sender = false): void {
  if (refuseWhileDaemonDown(sender ? "move the sender" : "move it")) return;
  openMailDialog({ kind: "move-to-mode", subject, sender });
}

interface SenderAskState {
  pending: MoveOutcome | null;
  set: (pending: MoveOutcome | null) => void;
}

export const useSenderAsk = create<SenderAskState>((set) => ({
  pending: null,
  set: (pending) => set({ pending }),
}));

const toastId = (outcome: MoveOutcome) => `move-${outcome.message_id}`;

export function answerPendingSenderAsk(): boolean {
  const pending = useSenderAsk.getState().pending;
  if (!pending) return false;
  void alwaysForSender(pending);
  return true;
}

export async function alwaysForSender(outcome: MoveOutcome): Promise<MoveOutcome | null> {
  useSenderAsk.getState().set(null);
  toast.dismiss(toastId(outcome));
  return performMove({ messageId: outcome.message_id, mode: outcome.to, sender: true });
}

export async function performMove(
  request: MoveRequest,
  options: { askInToast?: boolean } = {},
): Promise<MoveOutcome | null> {
  if (refuseWhileDaemonDown("move it")) return null;
  const claim = claimUndo();
  try {
    const outcome = await moveMessage(request);
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
    duration: ask ? 15_000 : UNDO_TOAST_DURATION_MS,
    description: hint ?? undefined,
    action: { label: createElement(UndoLabel), onClick: () => void undo() },
    cancel: ask
      ? { label: "Always for this sender", onClick: () => void alwaysForSender(outcome) }
      : undefined,
    onDismiss: clearAsk,
    onAutoClose: clearAsk,
  });
  return undo;
}
