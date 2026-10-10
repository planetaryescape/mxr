import { apiFetch } from "@/api/client";

export interface DraftSummary {
  id: string;
  revision?: number | null;
  account_id: string;
  subject: string;
  recipients: string;
  updated_at: string;
  updated_at_label: string;
  updated_at_full: string;
  updated_at_relative: string;
  attachment_count: number;
  content_kind: "markdown" | "html" | string;
  inline_asset_count: number;
  /** Set when the draft is scheduled to send. */
  send_at?: string | null;
}

export function fetchDrafts(): Promise<{ drafts: DraftSummary[] }> {
  return apiFetch<{ drafts: DraftSummary[] }>("/api/v1/mail/drafts");
}

export function deleteDraft(draft: DraftSummary): Promise<{ ok: boolean }> {
  if (draft.revision == null)
    return Promise.reject(
      new Error("Daemon does not support durable draft revisions; upgrade before deleting"),
    );
  return apiFetch<{ ok: boolean }>(
    `/api/v1/mail/drafts/${draft.id}/stored?expected_revision=${draft.revision}`,
    {
      method: "DELETE",
    },
  );
}

/** A stored draft stuck in `sending` (the send never confirmed). Raw daemon
 * `Draft` shape: the bridge passes the IPC response through. */
export interface OrphanedDraft {
  id: string;
  account_id: string;
  subject: string;
  to: { name: string | null; email: string }[];
  updated_at: string;
}

export async function fetchOrphanedDrafts(): Promise<OrphanedDraft[]> {
  const response = await apiFetch<{ drafts?: OrphanedDraft[] }>("/api/v1/mail/drafts/orphaned");
  return response.drafts ?? [];
}

/** Move an orphan back to an editable draft. Idempotent on the daemon. */
export function resetOrphanedDraft(draftId: string): Promise<unknown> {
  return apiFetch<unknown>(`/api/v1/mail/drafts/${encodeURIComponent(draftId)}/reset-orphan`, {
    method: "POST",
  });
}

export function sendStoredDraft(draftId: string): Promise<unknown> {
  return apiFetch<unknown>(`/api/v1/mail/drafts/${encodeURIComponent(draftId)}/send-stored`, {
    method: "POST",
  });
}

/** A pending scheduled send (`GET /api/v1/mail/scheduled-sends`), soonest first. */
export interface ScheduledSend {
  draft_id: string;
  account_id: string;
  send_at: string;
  subject: string;
  to: { name: string | null; email: string }[];
  cc: { name: string | null; email: string }[];
  bcc: { name: string | null; email: string }[];
  last_attempt_at: string | null;
  last_attempt_outcome: "sent" | "blocked" | "failed" | "interrupted" | null;
}

export async function fetchScheduledSends(account: string | null): Promise<ScheduledSend[]> {
  const query = account ? `?account=${encodeURIComponent(account)}` : "";
  const response = await apiFetch<{ sends?: ScheduledSend[] }>(
    `/api/v1/mail/scheduled-sends${query}`,
  );
  return response.sends ?? [];
}

/** Cancel the scheduled send; the draft itself is kept. */
export function cancelScheduledSend(draftId: string): Promise<unknown> {
  return apiFetch<unknown>(`/api/v1/mail/scheduled-sends/${encodeURIComponent(draftId)}`, {
    method: "DELETE",
  });
}
