/*
 * Turning a partial or failed mutation response into an error, and toasting
 * it with a re-authorize shortcut when the failure looks like expired auth.
 */

import { createElement } from "react";
import { toast } from "sonner";
import { UNDO_TOAST_DURATION_MS, UndoLabel } from "@/components/ui/sonner";

import { requestAccountReauth } from "@/features/accounts/reauthRequest";
import type { AccountMutationResult, MutationResponse } from "@/features/mailbox/types";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { plural } from "@/lib/format";
import { verb } from "./actionPastTense";
import type { MailAction, MailActionPayload } from "./pendingMailOps";

class MutationFailureError extends Error {
  constructor(
    message: string,
    readonly response: MutationResponse,
  ) {
    super(message);
    this.name = "MutationFailureError";
  }
}

export function assertCompleted(response: MutationResponse, requested: number): void {
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

/** The daemon's answer behind a partial or failed mutation, if there was one. */
export function failedResponse(error: Error): MutationResponse | null {
  return error instanceof MutationFailureError ? error.response : null;
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

/**
 * Say it failed, or partly failed when some messages changed. With `undo`
 * (the daemon kept one for what changed), the toast offers it, as `u` does.
 */
export function announceFailure(
  action: MailAction,
  error: Error,
  payload?: MailActionPayload,
  undo?: (() => Promise<boolean>) | null,
): void {
  const account = reauthableAccount(error);
  const result = failedResponse(error)?.result;
  const done = verb(action, payload);
  // "Archived 2 of 3 messages; the rest failed", or "Archived failed".
  const partial = Boolean(result && result.succeeded > 0);
  const title = partial
    ? `${done} ${result!.succeeded} of ${plural(result!.requested, "message")}; the rest failed`
    : `${done} failed`;
  // Part done is a warning (amber); nothing done is an error (red).
  toast[partial ? "warning" : "error"](title, {
    description: undo
      ? `${error.message.replace(/\.?$/, ".")} Press u to undo what changed.`
      : error.message,
    duration: undo ? UNDO_TOAST_DURATION_MS : undefined,
    action: undo
      ? { label: createElement(UndoLabel), onClick: () => void undo() }
      : account
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
