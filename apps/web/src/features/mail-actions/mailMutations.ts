import { usePendingMailOps, type MailAction, type MailActionPayload } from "./pendingMailOps";
import { playMailActionSound } from "@/features/sound/feedback";
import type { MutationResponse } from "@/features/mailbox/types";
import { isDaemonDown, refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { requestCoordinator } from "@/lib/requestCoordinator";
import { useSelection } from "@/state/selectionStore";
import { runAction } from "./mailActionRequests";
import { JOB_THRESHOLD, jobCommand, runAsJob } from "./mailMutationJobs";
import { invalidateMailQueries } from "./mailQueryInvalidation";
import { announceSuccess, claimUndo, keepPartialUndo } from "./mailUndo";
import { announceFailure, assertCompleted, failedResponse } from "./mutationFailure";

export type { MailAction, MailActionPayload };
export { invalidateMailQueries } from "./mailQueryInvalidation";
export { performUndo } from "./mailUndo";

export interface MailMutationOptions {
  payload?: MailActionPayload;
  silent?: boolean;
  until?: string;
}

export interface MailMutationOutcome {
  ok: boolean;
  response?: MutationResponse;
  error?: Error;
}

let opSequence = 0;
const archivesInFlight = new Set<string>();

export async function performMailAction(
  action: MailAction,
  messageIds: string[],
  options: MailMutationOptions = {},
): Promise<MailMutationOutcome> {
  const ids = [...new Set(messageIds)].filter(
    (id) => action !== "archive" || !archivesInFlight.has(id),
  );
  if (ids.length === 0) return { ok: false };
  if (options.silent ? isDaemonDown() : refuseMailActionWhileDaemonDown(action)) {
    return { ok: false };
  }
  if (action === "archive") for (const id of ids) archivesInFlight.add(id);
  const opId = `op-${Date.now()}-${(opSequence += 1)}`;
  const pending = usePendingMailOps.getState();
  if (action !== "archive")
    pending.add({ id: opId, action, messageIds: new Set(ids), payload: options.payload });
  if (isDestructive(action)) useSelection.getState().clear();
  const claim = options.silent ? null : claimUndo();

  try {
    const command = jobCommand(action, ids, options.payload);
    const response =
      action !== "archive" && command && ids.length >= JOB_THRESHOLD
        ? await runAsJob(action, command, ids.length, options.payload)
        : await requestCoordinator.enqueueMutation(() => runAction(action, ids, options));
    assertCompleted(response, ids.length);
    if (action === "archive") await invalidateMailQueries().catch(() => undefined);
    if (claim) claim.settle(announceSuccess(action, ids, response, claim.run, options.payload));
    if (!options.silent) playMailActionSound(action);
    if (action !== "archive") await invalidateMailQueries().catch(() => undefined);
    return { ok: true, response };
  } catch (caught) {
    const error = caught instanceof Error ? caught : new Error(String(caught));
    usePendingMailOps.getState().remove(opId);
    const response = failedResponse(error);
    if (action === "archive") await invalidateMailQueries().catch(() => undefined);
    const undo = claim && response ? keepPartialUndo(response, claim.run) : null;
    claim?.settle(undo);
    announceFailure(action, error, options.payload, undo, ids.length);
    if (action !== "archive") void invalidateMailQueries().catch(() => undefined);
    return { ok: false, error, response: response ?? undefined };
  } finally {
    if (action === "archive") for (const id of ids) archivesInFlight.delete(id);
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

export function isDestructive(action: MailAction): boolean {
  return DESTRUCTIVE.has(action);
}

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

export function refuseMailActionWhileDaemonDown(action: MailAction): boolean {
  return refuseWhileDaemonDown(REFUSED_WHILE_DOWN[action] ?? action);
}
