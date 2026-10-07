/*
 * Freshness over the bridge (`GetFreshness`): the newest arrival, each
 * account's sync health and the last arrivals with their modes. Fetched
 * at the rail's pace and refreshed live by the daemon's own events, so a
 * new message or a failed sync shows without polling faster.
 */

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
    // The rail's interval: events carry the news, this only catches drift.
    refetchInterval: 60_000,
  };
}

/** Freshness for the account scope the user picked. */
export function useFreshnessQuery() {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery(freshnessQuery(account));
}

/** Every account's freshness, for surfaces that list accounts. */
export function useAllAccountsFreshness() {
  return useQuery(freshnessQuery(null));
}

/** Events after which the newest arrival or a sync state may have moved. */
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

/** Refetch freshness when the daemon says mail arrived or a sync ended. */
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
