import { apiFetch } from "@/api/client";

export interface DraftSummary {
  id: string;
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
}

export function fetchDrafts(): Promise<{ drafts: DraftSummary[] }> {
  return apiFetch<{ drafts: DraftSummary[] }>("/api/v1/mail/drafts");
}

export function deleteDraft(draftId: string): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>(`/api/v1/mail/drafts/${draftId}/stored`, {
    method: "DELETE",
  });
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
