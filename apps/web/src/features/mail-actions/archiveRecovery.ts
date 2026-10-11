import { apiFetch } from "@/api/client";
import { getBridgeBaseUrl } from "@/lib/tokenStorage";
import type { components } from "@/api/generated";
import type { MutationResponse } from "@/features/mailbox/types";

export type ArchiveOperation = components["schemas"]["ArchiveOperationData"];
type Workspace = components["schemas"]["ArchiveWorkspaceData"];
type Identity = components["schemas"]["ArchiveIdentityData"];
const PREFIX = "mxr.archive.operation.";
const CALLER = "mxr.archive.caller";
export const ARCHIVE_JOURNAL_CHANGED = "mxr-archive-journal-changed";
const STATES = new Set(["pending", "unknown", "provider_succeeded", "completed", "not_executed"]);
function object(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}
function isOperation(value: unknown): value is ArchiveOperation {
  if (!object(value) || !object(value.identity) || !object(value.identity.workspace)) return false;
  const identity = value.identity;
  const workspace = value.identity.workspace;
  return (
    typeof identity.operation_id === "string" &&
    typeof identity.caller_id === "string" &&
    typeof workspace.workspace_id === "string" &&
    typeof workspace.recovery_generation === "string" &&
    typeof value.review_id === "string" &&
    typeof value.mutation_id === "string" &&
    typeof value.accepted === "boolean" &&
    typeof value.running === "boolean" &&
    typeof value.undo_available === "boolean" &&
    typeof value.started_at === "number" &&
    Array.isArray(value.items) &&
    value.items.every(
      (item: unknown) =>
        object(item) &&
        typeof item.message_id === "string" &&
        typeof item.account_id === "string" &&
        typeof item.provider_id === "string" &&
        typeof item.was_inbox === "boolean" &&
        typeof item.prior_flags_bits === "number" &&
        Array.isArray(item.prior_label_provider_ids) &&
        item.prior_label_provider_ids.every((label: unknown) => typeof label === "string") &&
        typeof item.state === "string" &&
        STATES.has(item.state) &&
        (item.reason == null || typeof item.reason === "string"),
    )
  );
}
function journalPrefix(): string {
  return `${PREFIX}${encodeURIComponent(getBridgeBaseUrl())}.`;
}
export function retainedArchives(): ArchiveOperation[] {
  const operations: ArchiveOperation[] = [];
  for (let index = 0; index < localStorage.length; index += 1) {
    const key = localStorage.key(index);
    if (!key?.startsWith(journalPrefix())) continue;
    const raw = localStorage.getItem(key);
    const value: unknown = raw ? JSON.parse(raw) : null;
    if (!isOperation(value))
      throw new Error(
        "Archive recovery journal is malformed; retain it and query the original identity",
      );
    operations.push(value);
  }
  return operations.toSorted((a, b) => a.started_at - b.started_at);
}
export function retainArchive(operation: ArchiveOperation): void {
  const key = `${journalPrefix()}${operation.identity.operation_id}`;
  const prior = localStorage.getItem(key);
  if (prior) {
    const stored: unknown = JSON.parse(prior);
    if (
      !isOperation(stored) ||
      JSON.stringify(stored.identity) !== JSON.stringify(operation.identity) ||
      stored.review_id !== operation.review_id
    ) {
      throw new Error(
        "Archive identity or review differs from the retained journal; query the original workspace",
      );
    }
  }
  localStorage.setItem(
    `${journalPrefix()}${operation.identity.operation_id}`,
    JSON.stringify(operation),
  );
  window.dispatchEvent(new Event(ARCHIVE_JOURNAL_CHANGED));
}
function callerId(): string {
  const existing = localStorage.getItem(CALLER);
  if (existing) return existing;
  const id = crypto.randomUUID();
  localStorage.setItem(CALLER, id);
  return id;
}
async function getWorkspace(): Promise<Workspace> {
  const response = await apiFetch<{ kind: string; workspace: Workspace }>(
    "/api/v1/mail/archive-operations/workspace",
  );
  if (response.kind !== "ArchiveWorkspace")
    throw new Error("Daemon does not support recoverable Archive");
  return response.workspace;
}
export async function previewArchive(ids: string[]): Promise<ArchiveOperation> {
  const identity: Identity = {
    operation_id: crypto.randomUUID(),
    caller_id: callerId(),
    workspace: await getWorkspace(),
  };
  const response = await apiFetch<{ kind: string; operation: ArchiveOperation }>(
    "/api/v1/mail/archive-operations/preview",
    {
      method: "POST",
      body: {
        identity,
        mutation: { mutation: "Archive", message_ids: [...new Set(ids)].toSorted() },
      },
    },
  );
  if (response.kind !== "ArchiveOperation" || !isOperation(response.operation))
    throw new Error("Invalid Archive review response");
  retainArchive(response.operation);
  return response.operation;
}
export async function queryArchive(operation: ArchiveOperation): Promise<ArchiveOperation> {
  const response = await apiFetch<{ kind: string; operation: ArchiveOperation }>(
    "/api/v1/mail/archive-operations/query",
    { method: "POST", body: { identity: operation.identity } },
  );
  if (response.kind !== "ArchiveOperation" || !isOperation(response.operation))
    throw new Error("Invalid Archive receipt response");
  retainArchive(response.operation);
  return response.operation;
}
export async function submitArchive(operation: ArchiveOperation): Promise<ArchiveOperation> {
  // Retention is part of submission, including a user-triggered retry from recovery.
  retainArchive(operation);
  const response = await apiFetch<{ kind: string; operation: ArchiveOperation }>(
    "/api/v1/mail/archive-operations/submit",
    {
      method: "POST",
      body: {
        identity: operation.identity,
        review_id: operation.review_id,
        mutation: {
          mutation: "Archive",
          message_ids: operation.items.map((item) => item.message_id),
        },
      },
    },
  );
  if (response.kind !== "ArchiveOperation" || !isOperation(response.operation))
    throw new Error("Invalid Archive acceptance response");
  retainArchive(response.operation);
  return response.operation;
}
export async function archiveMessagesRecoverably(ids: string[]): Promise<MutationResponse> {
  const operation = await previewArchive(ids);
  if (
    !window.confirm(
      `Archive ${operation.items.length} reviewed messages?\n${operation.items.map((item) => item.message_id).join("\n")}`,
    )
  )
    throw new Error("Archive review retained; submission cancelled");
  async function waitForOutcome(outcome: ArchiveOperation): Promise<ArchiveOperation> {
    if (!outcome.running) return outcome;
    await new Promise<void>((resolve) => window.setTimeout(resolve, 100));
    return waitForOutcome(await queryArchive(operation));
  }
  const outcome = await waitForOutcome(await submitArchive(operation));
  const succeeded = outcome.items.filter((item) => item.state === "completed").length;
  const incomplete = outcome.items.filter((item) => item.state !== "completed");
  const error =
    incomplete.length > 0
      ? `Archive operation ${outcome.identity.operation_id}: ${succeeded} confirmed complete; ${incomplete.map((item) => `${item.message_id} ${item.state}: ${item.reason ?? "Query again"}`).join("; ")}. Unknown effects may have applied. Open Archive recovery; do not create another operation.`
      : undefined;
  return {
    ok: incomplete.length === 0,
    error,
    result: {
      requested: outcome.items.length,
      succeeded,
      skipped: 0,
      failed: incomplete.length,
      accounts: [...new Set(outcome.items.map((item) => item.account_id))].map((accountId) => {
        const items = outcome.items.filter((item) => item.account_id === accountId);
        return {
          account_id: accountId,
          account_name: accountId,
          succeeded: items.filter((item) => item.state === "completed").length,
          skipped: 0,
          failed: items.filter((item) => item.state !== "completed").length,
          error:
            items
              .filter((item) => item.state !== "completed")
              .map((item) => `${item.state}: ${item.reason ?? "query retained operation"}`)
              .join("; ") || undefined,
        };
      }),
      mutation_id: outcome.undo_available ? outcome.mutation_id : undefined,
    },
  };
}
