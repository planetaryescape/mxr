/*
 * Undo for mail mutations: the success toast with its Undo, the early-`u`
 * claim on the undo slot, and the reversals (by mutation id, by batch job
 * chunk, or by waking snoozed messages).
 */

import { toast } from "sonner";

import { undoMutation, unsnoozeMessage } from "@/features/mailbox/api";
import type { MutationResponse } from "@/features/mailbox/types";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { plural } from "@/lib/format";
import { useUndo } from "@/state/undoStore";
import { verb } from "./actionPastTense";
import { invalidateMailQueries } from "./mailQueryInvalidation";
import type { MailAction, MailActionPayload } from "./pendingMailOps";

/** Undo by mutation id, then refresh every view that showed the change. */
export async function performUndo(mutationId: string): Promise<boolean> {
  if (refuseWhileDaemonDown("undo")) return false;
  try {
    await undoMutation(mutationId);
    const undo = useUndo.getState();
    if (undo.lastMutationId === mutationId) undo.recordNoUndo();
    toast.success("Undone");
    await invalidateMailQueries();
    return true;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (isRetryableUndo(message)) {
      offerRetry([mutationId], message);
      await invalidateMailQueries().catch(() => undefined);
    } else {
      toast.error("Undo failed", { description: message });
    }
    return false;
  }
}

/**
 * The daemon keeps what a partly failed undo could not restore under the
 * same id; undoing it again retries just those.
 */
function isRetryableUndo(message: string): boolean {
  return message.includes("can be retried with the same undo");
}

/** Say what is left, and hand `u` and the toast a retry of those ids. */
function offerRetry(undoIds: string[], description: string): void {
  const retry = async () => {
    useUndo.getState().retireUndo(retry);
    return undoIds.length === 1 ? performUndo(undoIds[0]!) : undoAll(undoIds);
  };
  useUndo.getState().recordUndo(retry, undoIds.at(-1));
  toast.error("Some messages weren't restored", {
    description,
    duration: 60_000,
    action: { label: "Retry", onClick: () => void retry() },
  });
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

/** "Archived 2 messages"; a snooze names the exact wake time it stored. */
function successMessage(action: MailAction, count: number, payload?: MailActionPayload): string {
  const until = action === "snooze" ? payload?.untilLabel : undefined;
  if (until) {
    return count === 1
      ? `Snoozed until ${until}`
      : `Snoozed ${plural(count, "message")} until ${until}`;
  }
  return `${verb(action, payload)} ${plural(count, "message")}`;
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
  const message = successMessage(action, count, payload);
  const mutationId = response.result?.mutation_id;
  const reverse = daemonReverse(response) ?? (action === "snooze" ? () => wakeSnoozed(ids) : null);
  // Running an undo retires it, from the key or the toast, so it can't run
  // twice; it only clears itself, never a newer action's undo.
  return offerUndo(message, `mutation-${mutationId ?? ids.join(",")}`, reverse, claim, mutationId);
}

/** The daemon's undo for a response: its mutation id, or every job chunk's. */
function daemonReverse(response: MutationResponse): (() => Promise<boolean>) | null {
  const mutationId = response.result?.mutation_id;
  const jobUndoIds = response.result?.undo_ids ?? [];
  if (mutationId) return () => performUndo(mutationId);
  return jobUndoIds.length > 0 ? () => undoAll(jobUndoIds) : null;
}

/**
 * A mutation that failed part way can still have changed some messages (or
 * may have, at the provider); the daemon keeps an undo for those. Hand the
 * claimed slot to it so `u` undoes what changed, and return it for the
 * failure toast. Null when the daemon kept none.
 */
export function keepPartialUndo(
  response: MutationResponse,
  claim: () => Promise<boolean>,
): (() => Promise<boolean>) | null {
  const reverse = daemonReverse(response);
  if (!reverse) return null;
  const undo = async () => {
    useUndo.getState().retireUndo(undo);
    return reverse();
  };
  if (useUndo.getState().lastUndo === claim) {
    useUndo.getState().recordUndo(undo, response.result?.mutation_id ?? undefined);
  }
  return undo;
}

/**
 * The success toast with its Undo, for any reversible change. Hands the
 * claimed undo slot to `reverse` if nothing newer took it; a change with no
 * reverse leaves the slot empty, so `u` never reaches past it.
 */
export function offerUndo(
  message: string,
  toastId: string,
  reverse: (() => Promise<boolean>) | null,
  claim: () => Promise<boolean>,
  mutationId?: string,
): (() => Promise<boolean>) | null {
  const undo = reverse
    ? async () => {
        useUndo.getState().retireUndo(undo!);
        // Its "Press u to undo" no longer applies.
        toast.dismiss(toastId);
        return reverse();
      }
    : null;
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
export async function undoAll(undoIds: string[]): Promise<boolean> {
  let ok = true;
  const retryable: string[] = [];
  let retryMessage = "";
  for (const id of undoIds.toReversed()) {
    try {
      // Chunks undo in reverse order, one at a time, like they were applied.
      // oxlint-disable-next-line no-await-in-loop
      await undoMutation(id);
    } catch (error) {
      ok = false;
      const message = error instanceof Error ? error.message : String(error);
      if (isRetryableUndo(message)) {
        retryable.unshift(id);
        retryMessage = message;
      }
    }
  }
  await invalidateMailQueries().catch(() => undefined);
  if (ok) toast.success("Undone");
  else if (retryable.length > 0) offerRetry(retryable, retryMessage);
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
