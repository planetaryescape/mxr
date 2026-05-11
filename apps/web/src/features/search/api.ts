import { apiFetch } from "@/api/client";
import type { MailboxResponse } from "@/features/mailbox/types";

export type SearchMode = "lexical" | "semantic" | "hybrid";
export type SearchSort = "relevance" | "newest" | "oldest";

export interface SearchParams {
  q: string;
  mode?: SearchMode;
  sort?: SearchSort;
  account?: string;
  limit?: number;
  offset?: number;
  scope?: "threads" | "messages" | "attachments";
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

export function fetchSearch(params: SearchParams): Promise<SearchResponse> {
  const query = new URLSearchParams();
  query.set("q", params.q);
  query.set("mode", params.mode ?? "lexical");
  query.set("sort", params.sort === "relevance" ? "relevant" : (params.sort ?? "newest"));
  query.set("limit", String(params.limit ?? 50));
  query.set("offset", String(params.offset ?? 0));
  query.set("scope", params.scope ?? "threads");
  if (params.account) query.set("account", params.account);
  return apiFetch<SearchResponse>(`/api/v1/mail/search?${query.toString()}`);
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
