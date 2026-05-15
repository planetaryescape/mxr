import { apiFetch } from "@/api/client";

export type ClientKind = "tui" | "cli" | "web" | "daemon";
export type ActivityTier = "ephemeral" | "standard" | "important";

export interface ActivityEntry {
  id: number;
  ts: number;
  account_id: string | null;
  source: ClientKind;
  action: string;
  target_kind: string | null;
  target_id: string | null;
  tier: ActivityTier;
  context: Record<string, unknown> | null;
  redacted: boolean;
}

export interface ActivityCursor {
  ts: number;
  id: number;
}

export interface ActivityListResponse {
  entries: ActivityEntry[];
  next_cursor: ActivityCursor | null;
}

export interface ActivityStatBucket {
  key: string;
  count: number;
}

export interface SavedActivityFilter {
  slug: string;
  name: string;
  filter: ActivityFilter;
  created_at: number;
  updated_at: number;
  last_used_at: number | null;
}

export interface ActivityFilter {
  since?: number | null;
  until?: number | null;
  account_id?: string | null;
  sources?: ClientKind[];
  actions?: string[];
  action_prefix?: string | null;
  target_kind?: string | null;
  target_id?: string | null;
  tiers?: ActivityTier[];
  query?: string | null;
  include_redacted?: boolean;
}

export interface FetchListParams {
  since?: number;
  until?: number;
  source?: ClientKind[];
  action?: string[];
  prefix?: string;
  tier?: ActivityTier[];
  query?: string;
  include_redacted?: boolean;
  limit?: number;
  cursor?: string;
}

function toQuery(params: FetchListParams): string {
  const search = new URLSearchParams();
  if (params.since !== undefined) search.set("since", params.since.toString());
  if (params.until !== undefined) search.set("until", params.until.toString());
  for (const s of params.source ?? []) search.append("source", s);
  for (const a of params.action ?? []) search.append("action", a);
  if (params.prefix) search.set("prefix", params.prefix);
  for (const t of params.tier ?? []) search.append("tier", t);
  if (params.query) search.set("query", params.query);
  if (params.include_redacted) search.set("include_redacted", "true");
  if (params.limit !== undefined) search.set("limit", params.limit.toString());
  if (params.cursor) search.set("cursor", params.cursor);
  return search.toString();
}

export function fetchActivityList(params: FetchListParams = {}) {
  const qs = toQuery(params);
  return apiFetch<ActivityListResponse>(`/api/v1/admin/activity${qs ? `?${qs}` : ""}`);
}

export function fetchActivityCount(params: FetchListParams = {}) {
  const qs = toQuery(params);
  return apiFetch<{ count: number }>(`/api/v1/admin/activity/count${qs ? `?${qs}` : ""}`);
}

export interface StatsParams {
  since: number;
  until: number;
  group_by: "action" | "day" | "source" | "target_kind" | "hour";
}

export function fetchActivityStats(p: StatsParams) {
  const qs = new URLSearchParams({
    since: p.since.toString(),
    until: p.until.toString(),
    group_by: p.group_by,
  }).toString();
  return apiFetch<{ buckets: ActivityStatBucket[] }>(`/api/v1/admin/activity/stats?${qs}`);
}

export async function redactActivity(
  ids: number[] | null,
  filter: ActivityFilter | null,
  dry_run: boolean,
) {
  return apiFetch<{ count: number; dry_run: boolean }>(
    "/api/v1/admin/activity/redact",
    {
      method: "POST",
      body: { ids: ids ?? [], filter: filter ?? null, dry_run },
    },
  );
}

export async function pauseActivity(until_ts: number | null) {
  return apiFetch<{ kind: string }>("/api/v1/admin/activity/pause", {
    method: "POST",
    body: { until_ts },
  });
}

export async function resumeActivity() {
  return apiFetch<{ kind: string }>("/api/v1/admin/activity/resume", {
    method: "POST",
    body: {},
  });
}

export async function exportActivity(filter: ActivityFilter, format: "csv" | "json" | "ndjson") {
  return apiFetch<{ count: number; size_bytes: number; body?: string; path?: string }>(
    "/api/v1/admin/activity/export",
    {
      method: "POST",
      body: { filter, format },
    },
  );
}

export async function listSavedFilters() {
  return apiFetch<{ entries: SavedActivityFilter[] }>("/api/v1/admin/activity/saved");
}

export async function upsertSavedFilter(slug: string, name: string, filter: ActivityFilter) {
  return apiFetch("/api/v1/admin/activity/saved", {
    method: "POST",
    body: { slug, name, filter },
  });
}

export async function deleteSavedFilter(slug: string) {
  return apiFetch(`/api/v1/admin/activity/saved/${encodeURIComponent(slug)}`, {
    method: "DELETE",
  });
}

export function formatTimestamp(ms: number): string {
  return new Date(ms).toLocaleString();
}
