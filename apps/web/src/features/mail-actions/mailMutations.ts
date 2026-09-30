/*
 * The one path every mail mutation takes, from any surface: key, button,
 * palette or bulk bar. It records a pending operation (instant UI), sends
 * the request through the serial mutation queue, reports the result with
 * an Undo, refreshes every mail view, and only then retires the pending
 * operation so the row never flickers back in between.
 */

import { usePendingMailOps, type MailAction, type MailActionPayload } from "./pendingMailOps";
import { playMailActionSound } from "@/features/sound/feedback";
import type { MutationResponse } from "@/features/mailbox/types";
import { isDaemonDown, refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { requestCoordinator } from "@/lib/requestCoordinator";
import { useSelection } from "@/state/selectionStore";
import { runAction } from "./mailActionRequests";
import { JOB_THRESHOLD, jobCommand, runAsJob } from "./mailMutationJobs";
import { invalidateMailQueries } from "./mailQueryInvalidation";
import { announceSuccess, claimUndo } from "./mailUndo";
import { announceFailure, assertCompleted } from "./mutationFailure";

export type { MailAction, MailActionPayload };
export { invalidateMailQueries } from "./mailQueryInvalidation";
export { performUndo } from "./mailUndo";

export interface MailMutationOptions {
  payload?: MailActionPayload;
  /** Skip the success toast (auto mark-read on open). */
  silent?: boolean;
  /** Snooze wake time (ISO), required for "snooze". */
  until?: string;
}

export interface MailMutationOutcome {
  ok: boolean;
  response?: MutationResponse;
  error?: Error;
}

let opSequence = 0;

export async function performMailAction(
  action: MailAction,
  messageIds: string[],
  options: MailMutationOptions = {},
): Promise<MailMutationOutcome> {
  const ids = [...new Set(messageIds)];
  if (ids.length === 0) return { ok: false };
  // A silent change (mark read on open) just doesn't happen yet.
  if (options.silent ? isDaemonDown() : refuseMailActionWhileDaemonDown(action)) {
    return { ok: false };
  }
  const opId = `op-${Date.now()}-${(opSequence += 1)}`;
  const pending = usePendingMailOps.getState();
  pending.add({ id: opId, action, messageIds: new Set(ids), payload: options.payload });
  if (isDestructive(action)) useSelection.getState().clear();
  // The row leaves the list at once, so `u` can come before the daemon
  // answers. Claim the undo slot now; an early `u` waits for the answer.
  const claim = options.silent ? null : claimUndo();

  try {
    const command = jobCommand(action, ids, options.payload);
    const response =
      command && ids.length >= JOB_THRESHOLD
        ? await runAsJob(action, command, ids.length, options.payload)
        : await requestCoordinator.enqueueMutation(() => runAction(action, ids, options));
    assertCompleted(response, ids.length);
    if (claim) claim.settle(announceSuccess(action, ids, response, claim.run, options.payload));
    if (!options.silent) playMailActionSound(action);
    await invalidateMailQueries().catch(() => undefined);
    return { ok: true, response };
  } catch (caught) {
    const error = caught instanceof Error ? caught : new Error(String(caught));
    // Retire the projection first so the rows come back, then reconcile:
    // the server may have applied part of the change before failing.
    usePendingMailOps.getState().remove(opId);
    claim?.settle(null);
    announceFailure(action, error, options.payload);
    void invalidateMailQueries().catch(() => undefined);
    return { ok: false, error };
  } finally {
    usePendingMailOps.getState().remove(opId);
  }
}

const DESTRUCTIVE = new Set<MailAction>([
  "archive",
  "trash",
  "spam",
  "read-and-archive",
  "move",
  "route",
  "label-remove",
  "snooze",
]);
// "labels" is destructive only when it removes the viewed label; the
// projection handles that without clearing the selection.

export function isDestructive(action: MailAction): boolean {
  return DESTRUCTIVE.has(action);
}

/** "Can't … while mxr's daemon is stopped", where it isn't the action's name. */
const REFUSED_WHILE_DOWN: Partial<Record<MailAction, string>> = {
  trash: "move to Trash",
  spam: "mark as spam",
  read: "mark as read",
  unread: "mark as unread",
  "read-and-archive": "mark read and archive",
  "label-add": "change labels",
  "label-remove": "change labels",
  labels: "change labels",
};

/**
 * No offline queue: a change the daemon can't take is refused up front,
 * before any row moves, so nothing on screen claims it happened.
 */
export function refuseMailActionWhileDaemonDown(action: MailAction): boolean {
  return refuseWhileDaemonDown(REFUSED_WHILE_DOWN[action] ?? action);
}
