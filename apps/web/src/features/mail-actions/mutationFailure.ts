/*
 * Turning a partial or failed mutation response into an error, and toasting
 * it with a re-authorize shortcut when the failure looks like expired auth.
 */

import { toast } from "sonner";

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

export function announceFailure(
  action: MailAction,
  error: Error,
  payload?: MailActionPayload,
): void {
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
