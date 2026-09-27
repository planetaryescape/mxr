import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

/** The bridge's `{ status }` body carries the daemon's snapshot as is. */
export type LlmStatus = components["schemas"]["LlmStatusSnapshot"];

export function fetchLlmStatus(): Promise<{ status: LlmStatus }> {
  return apiFetch<{ status: LlmStatus }>("/api/v1/platform/llm/status");
}

/**
 * Whether the daemon has a language model. AI features check this first,
 * so with no model configured they explain how to set one up instead of
 * failing on every thread.
 */
export const llmStatusQuery = {
  queryKey: ["llm-status"],
  queryFn: fetchLlmStatus,
  staleTime: 5 * 60_000,
};

export function useLlmStatus() {
  const query = useQuery(llmStatusQuery);
  return { ...query, enabled: query.data?.status.enabled === true };
}

/**
 * What decides which model writes model text and what it may read. Cached
 * model output (the thread gist) is keyed on it, so a settings change never
 * shows an answer written under the old policy.
 */
export function llmPolicyKey(status: LlmStatus | undefined): string {
  if (!status?.enabled) return "off";
  return [
    status.provider,
    status.model,
    status.base_url ?? "",
    status.allow_cloud_relationship_data ? "share" : "private",
  ].join("|");
}
