import { assertWorkspaceBinding, sameOwner, type WorkspaceBinding } from "@/lib/workspaceOwner";
import { apiFetch } from "@/api/client";

export interface SendReference {
  binding?: WorkspaceBinding;
  operationId: string;
  accountId: string;
  draftId: string;
}
export interface SendOutcome {
  identity: { operation_id: string; expected_revision: number };
  state: "unknown" | "accepted" | "not_accepted" | "waiting";
  local_message_id: string | null;
  recovery?: {
    retry_at: string | null;
    attempts: number;
    phase: string;
    reason: string | null;
  } | null;
}
const STORAGE_KEY = "mxr-reviewed-sends";
function isReference(value: unknown): value is SendReference {
  return (
    typeof value === "object" &&
    value !== null &&
    "operationId" in value &&
    typeof value.operationId === "string" &&
    "accountId" in value &&
    typeof value.accountId === "string" &&
    "draftId" in value &&
    typeof value.draftId === "string"
  );
}
export function readSendReferences(): SendReference[] {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) return [];
  try {
    const value: unknown = JSON.parse(raw);
    return Array.isArray(value) ? value.filter(isReference) : [];
  } catch {
    return [];
  }
}
/** Retain the query identity before the HTTP request can have effects. */
export function retainSendReference(reference: SendReference): void {
  const references = readSendReferences().filter(
    (r) =>
      r.operationId !== reference.operationId ||
      JSON.stringify(r.binding) !== JSON.stringify(reference.binding),
  );
  localStorage.setItem(STORAGE_KEY, JSON.stringify([...references, reference]));
}
export function fetchSendOutcome(
  accountId: string,
  operationId: string,
  binding?: WorkspaceBinding,
): Promise<{ outcome: SendOutcome | null }> {
  if (!binding)
    return Promise.reject(
      new Error(
        "Retained outcome has no workspace identity; reopen its original workspace and review explicitly",
      ),
    );
  assertWorkspaceBinding(binding);
  return apiFetch(
    `/api/v1/mail/send-outcomes/${encodeURIComponent(operationId)}?account_id=${encodeURIComponent(accountId)}`,
    { binding },
  );
}

export function recoverSendOutcome(
  reference: SendReference,
  evidenceSha256?: string,
): Promise<{
  outcome: SendOutcome;
  recoverable?: boolean;
  reason?: string;
  evidence_sha256?: string | null;
}> {
  if (!reference.binding)
    return Promise.reject(new Error("Recovery requires the original workspace owner"));
  assertWorkspaceBinding(reference.binding);
  return apiFetch(
    `/api/v1/mail/send-outcomes/${encodeURIComponent(reference.operationId)}/recover?account_id=${encodeURIComponent(reference.accountId)}`,
    {
      method: "POST",
      binding: reference.binding,
      body: {
        dry_run: evidenceSha256 === undefined,
        expected_evidence_sha256: evidenceSha256 ?? null,
      },
    },
  );
}

/** Called only by a deliberate Send press. Unknown acceptance never mints an attempt. */
export function operationAfterReview(defaultOperation: string, prior: SendOutcome | null): string {
  if (!prior) return defaultOperation;
  if (prior.state === "waiting")
    throw new Error(
      "This approved send is waiting for the provider and will retry automatically. Do not resend.",
    );
  if (prior.state === "unknown")
    throw new Error("Acceptance is uncertain. Check the send outcome; do not resend.");
  if (prior.state === "accepted") throw new Error("Provider already accepted this send.");
  return `${defaultOperation}:retry:${crypto.randomUUID()}`;
}
export async function prepareReviewedSend(reference: SendReference): Promise<SendReference> {
  const previous = readSendReferences()
    .toReversed()
    .find((r) => r.draftId === reference.draftId && r.accountId === reference.accountId);
  if (!reference.binding)
    throw new Error(
      "This reviewed send has no retained workspace owner; review explicitly before sending",
    );
  assertWorkspaceBinding(reference.binding);
  if (!previous) return reference;
  if (
    !previous.binding ||
    previous.binding.target !== reference.binding.target ||
    !sameOwner(previous.binding.workspace, reference.binding.workspace)
  )
    throw new Error(
      "A send outcome is retained for another or unknown workspace; return to its original workspace before continuing",
    );
  const { outcome } = await fetchSendOutcome(
    previous.accountId,
    previous.operationId,
    previous.binding,
  );
  return { ...reference, operationId: operationAfterReview(reference.operationId, outcome) };
}
