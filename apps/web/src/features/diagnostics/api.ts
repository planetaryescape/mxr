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

export interface LogsParams {
  limit?: number;
  level?: string;
  search?: string;
}

export function fetchLogs(arg: number | LogsParams = 100) {
  const params: LogsParams = typeof arg === "number" ? { limit: arg } : arg;
  const search = new URLSearchParams();
  if (params.limit !== undefined) search.set("limit", String(params.limit));
  if (params.level) search.set("level", params.level);
  if (params.search) search.set("search", params.search);
  const qs = search.toString();
  return apiFetch<{ lines?: string[]; entries?: unknown[] }>(
    `/api/v1/admin/logs${qs ? `?${qs}` : ""}`,
  );
}

export interface EventLogEntry {
  timestamp: number;
  level: string;
  category: string;
  account_id?: string | null;
  message_id?: string | null;
  rule_id?: string | null;
  summary: string;
  details?: string | null;
}

export interface EventsParams {
  limit?: number;
  offset?: number;
  level?: string;
  category?: string;
  category_prefix?: string;
  since?: number;
  until?: number;
  search?: string;
}

export function fetchEvents(arg: number | EventsParams = 50) {
  const params: EventsParams = typeof arg === "number" ? { limit: arg } : arg;
  const search = new URLSearchParams();
  if (params.limit !== undefined) search.set("limit", String(params.limit));
  if (params.offset !== undefined) search.set("offset", String(params.offset));
  if (params.level) search.set("level", params.level);
  if (params.category) search.set("category", params.category);
  if (params.category_prefix) search.set("category_prefix", params.category_prefix);
  if (params.since !== undefined) search.set("since", String(params.since));
  if (params.until !== undefined) search.set("until", String(params.until));
  if (params.search) search.set("search", params.search);
  const qs = search.toString();
  return apiFetch<{ entries: EventLogEntry[] }>(`/api/v1/admin/events${qs ? `?${qs}` : ""}`);
}

export function fetchEventCount(params: EventsParams = {}) {
  const search = new URLSearchParams();
  if (params.level) search.set("level", params.level);
  if (params.category) search.set("category", params.category);
  if (params.category_prefix) search.set("category_prefix", params.category_prefix);
  if (params.since !== undefined) search.set("since", String(params.since));
  if (params.until !== undefined) search.set("until", String(params.until));
  if (params.search) search.set("search", params.search);
  const qs = search.toString();
  return apiFetch<{ count: number }>(`/api/v1/admin/events/count${qs ? `?${qs}` : ""}`);
}

export function fetchEventCategories() {
  return apiFetch<{ categories: string[] }>(`/api/v1/admin/events/categories`);
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
