/*
 * The one path every mail mutation takes, from any surface: key, button,
 * palette or bulk bar. It records a pending operation (instant UI), sends
 * the request through the serial mutation queue, reports the result with
 * an Undo, refreshes every mail view, and only then retires the pending
 * operation so the row never flickers back in between.
 */

import type { QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import {
  archiveMessages,
  markReadMessages,
  modifyLabels,
  moveMessagesToLabel,
  readAndArchiveMessages,
  routeMessages,
  snoozeMessage,
  spamMessages,
  starMessages,
  trashMessages,
  undoMutation,
  unsnoozeMessage,
} from "@/features/mailbox/api";
import { usePendingMailOps, type MailAction, type MailActionPayload } from "./pendingMailOps";
import type { AccountMutationResult, MutationResponse } from "@/features/mailbox/types";
import { requestAccountReauth } from "@/features/accounts/reauthRequest";
import { getActiveQueryClient } from "@/lib/queryClient";
import { requestCoordinator } from "@/lib/requestCoordinator";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { plural } from "@/lib/format";
import { useSelection } from "@/state/selectionStore";
import { useUndo } from "@/state/undoStore";

export type { MailAction, MailActionPayload };

/** Query families that show mail and must refresh after any mutation. */
const MAIL_QUERY_ROOTS = new Set([
  "mailbox",
  "thread",
  "shell",
  "search",
  "search-groups",
  "search-palette",
  "reply-queue",
  "owed",
  "snoozed",
  "saved-search-counts",
]);

/**
 * Refetch every mail view, active or cached. Resolves when the active ones
 * have the server's answer.
 */
export async function invalidateMailQueries(qc: QueryClient | null = getActiveQueryClient()) {
  if (!qc) return;
  await qc.invalidateQueries({
    predicate: (query) => MAIL_QUERY_ROOTS.has(String(query.queryKey[0])),
  });
}

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
  const opId = `op-${Date.now()}-${(opSequence += 1)}`;
  const pending = usePendingMailOps.getState();
  pending.add({ id: opId, action, messageIds: new Set(ids), payload: options.payload });
  if (isDestructive(action)) useSelection.getState().clear();

  try {
    const response = await requestCoordinator.enqueueMutation(() =>
      runAction(action, ids, options),
    );
    assertCompleted(response, ids.length);
    if (!options.silent) announceSuccess(action, ids, response, options.payload);
    await invalidateMailQueries().catch(() => undefined);
    return { ok: true, response };
  } catch (caught) {
    const error = caught instanceof Error ? caught : new Error(String(caught));
    // Retire the projection first so the rows come back, then reconcile:
    // the server may have applied part of the change before failing.
    usePendingMailOps.getState().remove(opId);
    announceFailure(action, error, options.payload);
    void invalidateMailQueries().catch(() => undefined);
    return { ok: false, error };
  } finally {
    usePendingMailOps.getState().remove(opId);
  }
}

/** Undo by mutation id, then refresh every view that showed the change. */
export async function performUndo(mutationId: string): Promise<boolean> {
  try {
    await undoMutation(mutationId);
    const undo = useUndo.getState();
    if (undo.lastMutationId === mutationId) undo.clear();
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

async function runAction(
  action: MailAction,
  ids: string[],
  options: MailMutationOptions,
): Promise<MutationResponse> {
  const payload = options.payload;
  switch (action) {
    case "archive":
      return archiveMessages(ids);
    case "trash":
      return trashMessages(ids);
    case "spam":
      return spamMessages(ids);
    case "star":
      return starMessages(ids, true);
    case "unstar":
      return starMessages(ids, false);
    case "read":
      return markReadMessages(ids, true);
    case "unread":
      return markReadMessages(ids, false);
    case "read-and-archive":
      return readAndArchiveMessages(ids);
    case "move":
      if (!payload?.label) throw new Error("Choose a label to move to");
      return moveMessagesToLabel(ids, payload.label);
    case "route":
      if (!payload?.label) throw new Error("Choose a label to route to");
      if (!payload.fromQueueLabel) throw new Error("Open a queue label before routing");
      return routeMessages({
        messageIds: ids,
        toLabel: payload.label,
        fromQueueLabel: payload.fromQueueLabel,
        archive: payload.archive ?? true,
      });
    case "label-add":
      if (!payload?.label) throw new Error("Choose a label");
      return modifyLabels(ids, [payload.label], []);
    case "label-remove":
      if (!payload?.label) throw new Error("Choose a label");
      return modifyLabels(ids, [], [payload.label]);
    case "labels":
      if (!payload?.add?.length && !payload?.remove?.length) throw new Error("No label changes");
      return modifyLabels(ids, payload.add ?? [], payload.remove ?? []);
    case "snooze":
      return snoozeAll(ids, options.until);
  }
}

/**
 * The bridge snoozes one message per request. Report partial failure in the
 * same shape as batch mutations so the caller's accounting stays honest.
 */
async function snoozeAll(ids: string[], until?: string): Promise<MutationResponse> {
  if (!until) throw new Error("Choose when to snooze until");
  const results = await Promise.allSettled(
    ids.map((messageId) => snoozeMessage({ messageId, until })),
  );
  const failed = results.filter((result) => result.status === "rejected");
  const firstError = failed[0]?.status === "rejected" ? failed[0].reason : null;
  return {
    ok: failed.length === 0,
    result: {
      requested: ids.length,
      succeeded: ids.length - failed.length,
      skipped: 0,
      failed: failed.length,
      accounts: firstError
        ? [
            {
              account_id: "",
              account_name: "snooze",
              succeeded: ids.length - failed.length,
              skipped: 0,
              failed: failed.length,
              error: firstError instanceof Error ? firstError.message : String(firstError),
            },
          ]
        : undefined,
    },
  };
}

class MutationFailureError extends Error {
  constructor(
    message: string,
    readonly response: MutationResponse,
  ) {
    super(message);
    this.name = "MutationFailureError";
  }
}

function assertCompleted(response: MutationResponse, requested: number): void {
  if (!response.ok) throw new MutationFailureError(describeFailure(response, requested), response);
  const result = response.result;
  if (!result) return;
  if (result.succeeded === result.requested && result.skipped === 0 && result.failed === 0) return;
  throw new MutationFailureError(describeFailure(response, requested), response);
}

function describeFailure(response: MutationResponse, requestedFallback: number): string {
  const result = response.result;
  if (!result) return "The daemon rejected the change.";
  const requested = result.requested || requestedFallback;
  const accountErrors =
    result.accounts
      ?.filter((account) => account.error)
      .map((account) => `${account.account_name}: ${account.error}`) ?? [];
  const summary =
    result.succeeded > 0
      ? `Only ${result.succeeded} of ${plural(requested, "message")} changed`
      : "No messages changed";
  return accountErrors.length > 0 ? `${summary}. ${accountErrors.join("; ")}` : `${summary}.`;
}

const AUTH_RECOVERY_ERROR =
  /oauth|auth|token|invalid_client|invalid_grant|no sync provider configured|no sync-capable accounts configured|account unavailable/i;

function reauthableAccount(error: Error): AccountMutationResult | null {
  if (!(error instanceof MutationFailureError)) return null;
  return (
    error.response.result?.accounts?.find(
      (account) => account.error && account.account_id && AUTH_RECOVERY_ERROR.test(account.error),
    ) ?? null
  );
}

function verb(action: MailAction, payload?: MailActionPayload): string {
  switch (action) {
    case "archive":
      return "Archived";
    case "trash":
      return "Moved to Trash";
    case "spam":
      return "Marked as spam";
    case "star":
      return "Starred";
    case "unstar":
      return "Unstarred";
    case "read":
      return "Marked read";
    case "unread":
      return "Marked unread";
    case "read-and-archive":
      return "Read and archived";
    case "move":
      return payload?.label ? `Moved to ${payload.label}` : "Moved";
    case "route":
      return payload?.label ? `Routed to ${payload.label}` : "Routed";
    case "label-add":
      return payload?.label ? `Labelled ${payload.label}` : "Labelled";
    case "label-remove":
      return payload?.label ? `Removed ${payload.label}` : "Label removed";
    case "labels":
      return labelChangeVerb(payload);
    case "snooze":
      return "Snoozed";
  }
}

function labelChangeVerb(payload?: MailActionPayload): string {
  const add = payload?.add ?? [];
  const remove = payload?.remove ?? [];
  if (add.length === 1 && remove.length === 0) return `Labelled ${add[0]}`;
  if (remove.length === 1 && add.length === 0) return `Removed ${remove[0]} from`;
  return "Updated labels on";
}

function announceSuccess(
  action: MailAction,
  ids: string[],
  response: MutationResponse,
  payload?: MailActionPayload,
): void {
  const count = response.result?.succeeded ?? ids.length;
  const message = `${verb(action, payload)} ${plural(count, "message")}`;
  const mutationId = response.result?.mutation_id;
  const undo = mutationId
    ? () => performUndo(mutationId)
    : action === "snooze"
      ? () => wakeSnoozed(ids)
      : null;
  if (!undo) {
    toast.success(message);
    return;
  }
  const undoState = useUndo.getState();
  if (mutationId) undoState.setLastMutationId(mutationId);
  undoState.setLastUndo(undo);
  toast.success(message, {
    id: `mutation-${mutationId ?? ids.join(",")}`,
    duration: 60_000,
    description: "Press u to undo",
    action: { label: "Undo", onClick: () => void undo() },
  });
}

/** Snooze has no daemon mutation id; undo wakes each message instead. */
async function wakeSnoozed(ids: string[]): Promise<boolean> {
  const results = await Promise.allSettled(ids.map((id) => unsnoozeMessage(id)));
  const failed = results.filter((result) => result.status === "rejected").length;
  useUndo.getState().setLastUndo(null);
  await invalidateMailQueries().catch(() => undefined);
  if (failed > 0) {
    toast.error(`Couldn't wake ${plural(failed, "message")}`);
    return false;
  }
  toast.success("Snooze undone");
  return true;
}

function announceFailure(action: MailAction, error: Error, payload?: MailActionPayload): void {
  const account = reauthableAccount(error);
  toast.error(`${verb(action, payload)} failed`, {
    description: error.message,
    action: account
      ? {
          label: `Re-authorize ${account.account_name}`,
          onClick: () => {
            requestAccountReauth(account.account_id);
            getRuntimeNavigate().navigate(`/accounts/${encodeURIComponent(account.account_id)}`);
          },
        }
      : undefined,
  });
}
