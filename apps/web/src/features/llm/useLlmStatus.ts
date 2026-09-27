import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";

export interface LlmStatus {
  enabled: boolean;
  provider: string;
  model: string;
  configured_model?: string | null;
}

export function fetchLlmStatus(): Promise<{ status: LlmStatus }> {
  return apiFetch<{ status: LlmStatus }>("/api/v1/platform/llm/status");
}

/**
 * Whether the daemon has a language model. AI features check this first,
 * so with no model configured they explain how to set one up instead of
 * failing on every thread.
 */
export function useLlmStatus() {
  const query = useQuery({
    queryKey: ["llm-status"],
    queryFn: fetchLlmStatus,
    staleTime: 5 * 60_000,
  });
  return { ...query, enabled: query.data?.status.enabled === true };
}
