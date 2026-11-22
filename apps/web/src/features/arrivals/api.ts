import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useMemo, useRef } from "react";

import { apiFetch } from "@/api/client";
import { useUiPrefs } from "@/state/uiPrefsStore";

import type {
  ArrivalBucket,
  ArrivalItem,
  ArrivalList,
  ArrivalListResponse,
  ArrivalModesResponse,
  Arrivals,
  ArrivalsResponse,
  MessageMovedResponse,
  ModeId,
  MoveOutcome,
  MoveUndoneResponse,
} from "./types";

export const ARRIVALS_KEY = ["arrivals"] as const;
export const ARRIVAL_MODES_KEY = ["arrival-modes"] as const;

export const ARRIVAL_MODES_MAX = 200;

export async function fetchArrivals(
  account: string | null,
  markSeen: boolean,
  since?: string,
): Promise<Arrivals> {
  const params = new URLSearchParams();
  if (account) params.set("account", account);
  if (markSeen) params.set("mark_seen", "true");
  else if (since) params.set("since", since);
  const query = params.size > 0 ? `?${params}` : "";
  const answer = await apiFetch<ArrivalsResponse>(`/api/v1/mail/arrivals${query}`);
  return answer.arrivals;
}

export function useArrivalsQuery() {
  const account = useUiPrefs((s) => s.accountScope);
  const visiting = useRef(true);
  const openVisit = useRef<{ account: string | null; since: string } | null>(null);
  return useQuery({
    queryKey: [...ARRIVALS_KEY, "line", account ?? "all"],
    queryFn: async () => {
      const markSeen = visiting.current;
      visiting.current = false;
      const held = openVisit.current?.account === account ? openVisit.current.since : undefined;
      const arrivals = await fetchArrivals(account, markSeen, held);
      if (markSeen || held === undefined) openVisit.current = { account, since: arrivals.since };
      return arrivals;
    },
    refetchOnMount: "always",
    staleTime: 15_000,
    refetchInterval: 60_000,
  });
}

export interface ArrivalListParams {
  bucket?: ArrivalBucket;
  since?: string;
  until?: string;
}

export async function fetchArrivalList(
  account: string | null,
  params: ArrivalListParams,
): Promise<ArrivalList> {
  const search = new URLSearchParams();
  if (account) search.set("account", account);
  if (params.bucket) search.set("bucket", params.bucket);
  if (params.since) search.set("since", params.since);
  if (params.until) search.set("until", params.until);
  search.set("limit", "500");
  const answer = await apiFetch<ArrivalListResponse>(`/api/v1/mail/arrivals/list?${search}`);
  return answer.list;
}

export function useArrivalList(params: ArrivalListParams) {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery({
    queryKey: [...ARRIVALS_KEY, "list", account ?? "all", params],
    queryFn: () => fetchArrivalList(account, params),
    placeholderData: keepPreviousData,
    staleTime: 15_000,
  });
}

export async function fetchArrivalModes(messageIds: readonly string[]): Promise<ArrivalItem[]> {
  const chunks: string[][] = [];
  for (let at = 0; at < messageIds.length; at += ARRIVAL_MODES_MAX) {
    chunks.push(messageIds.slice(at, at + ARRIVAL_MODES_MAX));
  }
  const answers = await Promise.all(
    chunks.map((chunk) =>
      apiFetch<ArrivalModesResponse>("/api/v1/mail/arrivals/modes", {
        method: "POST",
        body: { message_ids: chunk },
      }),
    ),
  );
  return answers.flatMap((answer) => answer.items);
}

export function useArrivalModes(messageIds: readonly string[]) {
  const sorted = useMemo(() => [...new Set(messageIds)].toSorted(), [messageIds]);
  return useQuery({
    queryKey: [...ARRIVAL_MODES_KEY, sorted],
    queryFn: async () => {
      const items = await fetchArrivalModes(sorted);
      return new Map(items.map((item) => [item.message_id, item]));
    },
    enabled: sorted.length > 0,
    placeholderData: keepPreviousData,
    staleTime: 30_000,
  });
}

export interface MoveRequest {
  messageId: string;
  mode: ModeId;
  sender?: boolean;
  source?: "not_sure";
  dryRun?: boolean;
}

export async function moveMessage(request: MoveRequest): Promise<MoveOutcome> {
  const body: { mode: ModeId; sender: boolean; dry_run: boolean; source?: "not_sure" } = {
    mode: request.mode,
    sender: request.sender ?? false,
    dry_run: request.dryRun ?? false,
  };
  if (request.source) body.source = request.source;
  const answer = await apiFetch<MessageMovedResponse>(
    `/api/v1/mail/messages/${encodeURIComponent(request.messageId)}/move`,
    { method: "POST", body },
  );
  return answer.outcome;
}

export async function undoMove(correctionId: number): Promise<string> {
  const answer = await apiFetch<MoveUndoneResponse>(
    `/api/v1/mail/moves/${encodeURIComponent(String(correctionId))}/undo`,
    { method: "POST" },
  );
  return answer.copy;
}
