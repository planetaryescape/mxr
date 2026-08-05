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
