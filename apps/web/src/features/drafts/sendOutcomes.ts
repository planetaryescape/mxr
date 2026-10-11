import { apiFetch } from "@/api/client";

export interface SendReference {
  operationId: string;
  accountId: string;
  draftId: string;
}
export interface SendOutcome {
  identity: { operation_id: string; expected_revision: number };
  state: "unknown" | "accepted" | "not_accepted";
  local_message_id: string | null;
}
const STORAGE_KEY = "mxr-reviewed-sends";
function isReference(value: unknown): value is SendReference {
  return typeof value === "object" && value !== null &&
    "operationId" in value && typeof value.operationId === "string" &&
    "accountId" in value && typeof value.accountId === "string" &&
    "draftId" in value && typeof value.draftId === "string";
}
export function readSendReferences(): SendReference[] {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) return [];
  try {
    const value: unknown = JSON.parse(raw);
    return Array.isArray(value) ? value.filter(isReference) : [];
  } catch { return []; }
}
/** Retain the query identity before the HTTP request can have effects. */
export function retainSendReference(reference: SendReference): void {
  const references = readSendReferences().filter((r) => r.operationId !== reference.operationId);
  localStorage.setItem(STORAGE_KEY, JSON.stringify([...references, reference]));
}
export function fetchSendOutcome(accountId: string, operationId: string): Promise<{ outcome: SendOutcome | null }> {
  return apiFetch(`/api/v1/mail/send-outcomes/${encodeURIComponent(operationId)}?account_id=${encodeURIComponent(accountId)}`);
}

/** Called only by a deliberate Send press. Unknown acceptance never mints an attempt. */
export function operationAfterReview(defaultOperation: string, prior: SendOutcome | null): string {
  if (!prior) return defaultOperation;
  if (prior.state === "unknown") throw new Error("Acceptance is uncertain. Check the send outcome; do not resend.");
  if (prior.state === "accepted") throw new Error("Provider already accepted this send.");
  return `${defaultOperation}:retry:${crypto.randomUUID()}`;
}
export async function prepareReviewedSend(reference: SendReference): Promise<SendReference> {
  const previous = readSendReferences().toReversed().find((r) => r.draftId === reference.draftId && r.accountId === reference.accountId);
  if (!previous) return reference;
  const { outcome } = await fetchSendOutcome(previous.accountId, previous.operationId);
  return { ...reference, operationId: operationAfterReview(reference.operationId, outcome) };
}
