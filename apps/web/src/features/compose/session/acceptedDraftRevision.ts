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
