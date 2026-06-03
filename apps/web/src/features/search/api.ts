import { apiFetch } from "@/api/client";
import type { MailboxResponse } from "@/features/mailbox/types";

export type SearchMode = "lexical" | "semantic" | "hybrid";
export type SearchSort = "relevance" | "newest" | "oldest" | "verdict";

export interface SearchParams {
  q: string;
  mode?: SearchMode;
  sort?: SearchSort;
  account?: string;
  limit?: number;
  offset?: number;
  scope?: "threads" | "messages" | "attachments" | "triage";
  verdict?: "ACTION" | "FYI" | "ROUTINE";
}

export interface SearchResponse {
  scope: string;
  sort: string;
  mode: SearchMode | string;
  total: number;
  has_more: boolean;
  next_offset?: number | null;
  groups: MailboxResponse["mailbox"]["groups"];
  explain?: unknown;
  llm_calls?: number;
  prompt_version?: string;
}

export interface SavedSearch {
  id: string;
  name: string;
  query: string;
  search_mode?: SearchMode | string;
  sort?: string;
  icon?: string | null;
  position?: number;
  created_at?: string;
}

export function searchKey(params: SearchParams) {
  return ["search", params] as const;
}

export function fetchSearch(
  params: SearchParams,
  opts: { signal?: AbortSignal } = {},
): Promise<SearchResponse> {
  const query = new URLSearchParams();
  query.set("q", params.q);
  query.set("mode", params.mode ?? "lexical");
  query.set("sort", params.sort === "relevance" ? "relevant" : (params.sort ?? "newest"));
  query.set("limit", String(params.limit ?? 50));
  query.set("offset", String(params.offset ?? 0));
  query.set("scope", params.scope ?? "threads");
  if (params.verdict) query.set("verdict", params.verdict);
  if (params.account) query.set("account", params.account);
  const path = params.scope === "triage" ? "/api/v1/mail/triage" : "/api/v1/mail/search";
  return apiFetch<SearchResponse>(`${path}?${query.toString()}`, {
    signal: opts.signal,
  });
}

export function fetchSavedSearches(): Promise<{ searches: SavedSearch[] }> {
  return apiFetch<{ searches: SavedSearch[] }>("/api/v1/platform/saved-searches");
}

export function createSavedSearch(input: { name: string; query: string; mode: SearchMode }) {
  return apiFetch<unknown>("/api/v1/platform/saved-searches/create", {
    method: "POST",
    body: { name: input.name, query: input.query, search_mode: input.mode },
  });
}

export function deleteSavedSearch(name: string) {
  return apiFetch<unknown>("/api/v1/platform/saved-searches/delete", {
    method: "POST",
    body: { name },
  });
}

export interface SavedSearchPatch {
  new_name?: string;
  query?: string;
  search_mode?: SearchMode;
  sort?: string;
  /** Hex color (#RRGGBB) — overloaded onto the protocol's `icon` field. */
  icon?: string;
  /** Smaller = higher in list. Negative values pin to top. */
  position?: number;
}

export function updateSavedSearch(name: string, patch: SavedSearchPatch) {
  return apiFetch<SavedSearch>("/api/v1/platform/saved-searches/update", {
    method: "POST",
    body: { name, ...patch },
  });
}
