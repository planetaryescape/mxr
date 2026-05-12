import { apiFetch } from "@/api/client";

export function fetchAdminStatus() {
  return apiFetch<Record<string, unknown>>("/api/v1/admin/status");
}

export function fetchDiagnostics() {
  return apiFetch<{ report: Record<string, unknown> }>("/api/v1/admin/diagnostics");
}

export function fetchBugReport() {
  return apiFetch<{ content: string }>("/api/v1/admin/diagnostics/bug-report");
}

export function fetchLogs(limit = 100) {
  return apiFetch<{ lines?: string[]; entries?: unknown[] }>(`/api/v1/admin/logs?limit=${limit}`);
}

export function fetchEvents(limit = 50) {
  return apiFetch<{ entries?: unknown[] }>(`/api/v1/admin/events?limit=${limit}`);
}

export function fetchSyncStatus(accountId: string) {
  return apiFetch<Record<string, unknown>>(
    `/api/v1/mail/sync/status?account_id=${encodeURIComponent(accountId)}`,
  );
}

export const semanticProfiles = ["bge-small-en-v1.5", "multilingual-e5-small", "bge-m3"] as const;

export type SemanticProfile = (typeof semanticProfiles)[number];

export interface SemanticProfileRecord {
  profile: SemanticProfile;
  backend: string;
  model_revision: string;
  dimensions: number;
  status: "pending" | "ready" | "indexing" | "error";
  installed_at?: string | null;
  activated_at?: string | null;
  last_indexed_at?: string | null;
  progress_completed: number;
  progress_total: number;
  last_error?: string | null;
}

export interface SemanticStatusSnapshot {
  enabled: boolean;
  active_profile: SemanticProfile;
  profiles: SemanticProfileRecord[];
  runtime?: {
    queue_depth?: number;
    in_flight?: number;
    last_queue_wait_ms?: number | null;
    last_extract_ms?: number | null;
    last_embedding_prep_ms?: number | null;
    last_ingest_ms?: number | null;
  };
}

type SemanticStatusResponse =
  | { status: SemanticStatusSnapshot }
  | { snapshot: SemanticStatusSnapshot }
  | SemanticStatusSnapshot;

export function fetchSemanticStatus() {
  return apiFetch<SemanticStatusResponse>("/api/v1/platform/semantic/status");
}

export function semanticSnapshot(response: SemanticStatusResponse | undefined) {
  if (!response) return null;
  if ("status" in response) return response.status;
  if ("snapshot" in response) return response.snapshot;
  return response;
}

export function setSemanticEnabled(enabled: boolean) {
  return apiFetch<SemanticStatusResponse>("/api/v1/platform/semantic/enable", {
    method: "POST",
    body: { enabled },
  });
}

export function reindexSemantic() {
  return apiFetch<Record<string, unknown>>("/api/v1/platform/semantic/reindex", {
    method: "POST",
  });
}

export function installSemanticProfile(profile: SemanticProfile) {
  return apiFetch<SemanticStatusResponse>("/api/v1/platform/semantic/profiles/install", {
    method: "POST",
    body: { profile },
  });
}

export function useSemanticProfile(profile: SemanticProfile) {
  return apiFetch<SemanticStatusResponse>("/api/v1/platform/semantic/profiles/use", {
    method: "POST",
    body: { profile },
  });
}

export function backfillSemantic() {
  return apiFetch<SemanticStatusResponse>("/api/v1/platform/semantic/backfill", {
    method: "POST",
  });
}
