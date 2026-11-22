import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";

import type { Freshness } from "./copy";
import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { useDaemonEvents, type DaemonEvent } from "@/hooks/useDaemonEvents";
import { useUiPrefs } from "@/state/uiPrefsStore";

type FreshnessResponse = Extract<components["schemas"]["ResponseData"], { kind: "Freshness" }>;

export const freshnessKey = ["freshness"] as const;

export async function fetchFreshness(account: string | null): Promise<Freshness> {
  const query = account ? `?account=${encodeURIComponent(account)}` : "";
  const answer = await apiFetch<FreshnessResponse>(`/api/v1/mail/freshness${query}`);
  return answer.freshness;
}

function freshnessQuery(account: string | null) {
  return {
    queryKey: [...freshnessKey, account ?? "all"],
    queryFn: () => fetchFreshness(account),
    staleTime: 15_000,
    refetchInterval: 60_000,
  };
}

export function useFreshnessQuery() {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery(freshnessQuery(account));
}

export function useAllAccountsFreshness() {
  return useQuery(freshnessQuery(null));
}

export function changesFreshness(event: DaemonEvent): boolean {
  switch (event.type) {
    case "NewMessages":
    case "SyncCompleted":
    case "SyncError":
    case "EventsLagged":
      return true;
    case "OperationCompleted":
    case "OperationFailed":
      return event.operation === "sync";
    default:
      return false;
  }
}

export function useFreshnessLive(): void {
  const qc = useQueryClient();
  useDaemonEvents(
    useCallback(
      (event: DaemonEvent) => {
        if (changesFreshness(event)) void qc.invalidateQueries({ queryKey: freshnessKey });
      },
      [qc],
    ),
  );
}
