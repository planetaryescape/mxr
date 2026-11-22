import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

export type LlmStatus = components["schemas"]["LlmStatusSnapshot"];

export function fetchLlmStatus(): Promise<{ status: LlmStatus }> {
  return apiFetch<{ status: LlmStatus }>("/api/v1/platform/llm/status");
}

export const llmStatusQuery = {
  queryKey: ["llm-status"],
  queryFn: fetchLlmStatus,
  staleTime: 5 * 60_000,
};

export function useLlmStatus() {
  const query = useQuery(llmStatusQuery);
  return { ...query, enabled: query.data?.status.enabled === true };
}

export function llmPolicyKey(status: LlmStatus | undefined): string {
  if (!status?.enabled) return "off";
  return [
    status.provider,
    status.model,
    status.policy_key ?? status.base_url ?? "",
    status.allow_cloud_relationship_data ? "share" : "private",
  ].join("|");
}
