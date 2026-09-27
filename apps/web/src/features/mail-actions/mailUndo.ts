/*
 * Undo for mail mutations: the success toast with its Undo, the early-`u`
 * claim on the undo slot, and the reversals (by mutation id, by batch job
 * chunk, or by waking snoozed messages).
 */

import { toast } from "sonner";

import { undoMutation, unsnoozeMessage } from "@/features/mailbox/api";
import type { MutationResponse } from "@/features/mailbox/types";
import { plural } from "@/lib/format";
import { useUndo } from "@/state/undoStore";
import { verb } from "./actionPastTense";
import { invalidateMailQueries } from "./mailQueryInvalidation";
import type { MailAction, MailActionPayload } from "./pendingMailOps";

/** Undo by mutation id, then refresh every view that showed the change. */
export async function performUndo(mutationId: string): Promise<boolean> {
  try {
    await undoMutation(mutationId);
    const undo = useUndo.getState();
    if (undo.lastMutationId === mutationId) undo.recordNoUndo();
    toast.success("Undone");
    await invalidateMailQueries();
    return true;
  } catch (error) {
    toast.error("Undo failed", {
      description: error instanceof Error ? error.message : String(error),
    });
    return false;
  }
}

interface UndoClaim {
  run: () => Promise<boolean>;
  settle: (undo: (() => Promise<boolean>) | null) => void;
}

/** Hold the newest-undo slot for an action whose answer hasn't come yet. */
export function claimUndo(): UndoClaim {
  let settle!: UndoClaim["settle"];
  const settled = new Promise<(() => Promise<boolean>) | null>((resolve) => {
    settle = resolve;
  });
  const run = async () => {
    useUndo.getState().retireUndo(run);
    const undo = await settled;
    return undo ? undo() : false;
  };
  useUndo.getState().recordUndo(run);
  return {
    run,
    settle: (undo) => {
      useUndo.getState().retireUndo(run);
      settle(undo);
    },
  };
}

/** Toast the result; returns its undo, recorded only if nothing newer took the slot. */
export function announceSuccess(
  action: MailAction,
  ids: string[],
  response: MutationResponse,
  claim: () => Promise<boolean>,
  payload?: MailActionPayload,
): (() => Promise<boolean>) | null {
  const count = response.result?.succeeded ?? ids.length;
  const message = `${verb(action, payload)} ${plural(count, "message")}`;
  const mutationId = response.result?.mutation_id;
  const jobUndoIds = response.result?.undo_ids ?? [];
  const reverse = mutationId
    ? () => performUndo(mutationId)
    : jobUndoIds.length > 0
      ? () => undoAll(jobUndoIds)
      : action === "snooze"
        ? () => wakeSnoozed(ids)
        : null;
  // Running an undo retires it, from the key or the toast, so it can't run
  // twice; it only clears itself, never a newer action's undo.
  const toastId = `mutation-${mutationId ?? ids.join(",")}`;
  const undo = reverse
    ? async () => {
        useUndo.getState().retireUndo(undo!);
        // Its "Press u to undo" no longer applies.
        toast.dismiss(toastId);
        return reverse();
      }
    : null;
  // Still the newest action (and `u` wasn't pressed early): hand the slot
  // over to the real undo. A change with no undo leaves the slot empty, so
  // `u` never reaches past it.
  const newest = useUndo.getState().lastUndo === claim;
  if (!undo) {
    if (newest) useUndo.getState().recordNoUndo();
    toast.success(message);
    return null;
  }
  if (newest) useUndo.getState().recordUndo(undo, mutationId);
  toast.success(message, {
    id: toastId,
    duration: 60_000,
    description: "Press u to undo",
    action: { label: "Undo", onClick: () => void undo() },
  });
  return undo;
}

/** A batch job's chunks each undo separately; reverse them all, newest first. */
async function undoAll(undoIds: string[]): Promise<boolean> {
  let ok = true;
  for (const id of undoIds.toReversed()) {
    try {
      // Chunks undo in reverse order, one at a time, like they were applied.
      // oxlint-disable-next-line no-await-in-loop
      await undoMutation(id);
    } catch {
      ok = false;
    }
  }
  await invalidateMailQueries().catch(() => undefined);
  if (ok) toast.success("Undone");
  else
    toast.error("Part of the batch couldn't be undone", {
      description: "Its undo window may have passed.",
    });
  return ok;
}

/** Snooze has no daemon mutation id; undo wakes each message instead. */
async function wakeSnoozed(ids: string[]): Promise<boolean> {
  const results = await Promise.allSettled(ids.map((id) => unsnoozeMessage(id)));
  const failed = results.filter((result) => result.status === "rejected").length;
  await invalidateMailQueries().catch(() => undefined);
  if (failed > 0) {
    toast.error(`Couldn't wake ${plural(failed, "message")}`);
    return false;
  }
  toast.success("Snooze undone");
  return true;
}
