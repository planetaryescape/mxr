import { BridgeRequestError, DaemonUnavailableError } from "@/api/client";
import type { components } from "@/api/generated";

/** Only this request's confirmed local persistence may advance its editor token. */
export function acceptedDraftRevision(
  error: unknown,
): components["schemas"]["AcceptedComposeDraft"] | undefined {
  const failure = error instanceof DaemonUnavailableError ? error.cause : error;
  if (!(failure instanceof BridgeRequestError)) return undefined;
  const accepted = failure.details.accepted_draft;
  if (
    !accepted ||
    typeof accepted !== "object" ||
    !("draft_id" in accepted) ||
    !("revision" in accepted)
  )
    return undefined;
  if (
    typeof accepted.draft_id !== "string" ||
    !Number.isSafeInteger(accepted.revision) ||
    typeof accepted.revision !== "number" ||
    accepted.revision < 1
  )
    return undefined;
  return { draft_id: accepted.draft_id, revision: accepted.revision };
}

export function recoveredComposeSession(
  error: unknown,
): components["schemas"]["RecoveredComposeSession"] | undefined {
  const failure = error instanceof DaemonUnavailableError ? error.cause : error;
  if (!(failure instanceof BridgeRequestError)) return undefined;
  const recovery = failure.details.recovery_session;
  if (
    !recovery ||
    typeof recovery !== "object" ||
    !("draft_path" in recovery) ||
    !("draft_id" in recovery) ||
    !("revision" in recovery)
  )
    return undefined;
  if (
    typeof recovery.draft_path !== "string" ||
    !recovery.draft_path ||
    typeof recovery.draft_id !== "string"
  )
    return undefined;
  const revision = recovery.revision;
  if (
    revision !== null &&
    (typeof revision !== "number" || !Number.isSafeInteger(revision) || revision < 1)
  )
    return undefined;
  return { draft_path: recovery.draft_path, draft_id: recovery.draft_id, revision };
}
