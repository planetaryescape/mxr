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

export function fetchSemanticStatus() {
  return apiFetch<Record<string, unknown>>("/api/v1/platform/semantic/status");
}

export function backfillSemantic() {
  return apiFetch<Record<string, unknown>>("/api/v1/platform/semantic/backfill", {
    method: "POST",
  });
}
