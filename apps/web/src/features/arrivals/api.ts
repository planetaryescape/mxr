/*
 * Sorting shows its work (D119) over the bridge: Now's arrivals line, the
 * emails behind each count, Inbox's mode chips, moving an email or its
 * sender, and undo. The daemon owns the window, the counts and every word.
 */

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

/** The most emails one chip request may name (the daemon's cap). */
export const ARRIVAL_MODES_MAX = 200;

export async function fetchArrivals(account: string | null, markSeen: boolean): Promise<Arrivals> {
  const params = new URLSearchParams();
  if (account) params.set("account", account);
  if (markSeen) params.set("mark_seen", "true");
  const query = params.size > 0 ? `?${params}` : "";
  const answer = await apiFetch<ArrivalsResponse>(`/api/v1/mail/arrivals${query}`);
  return answer.arrivals;
}

/**
 * The line on Now. The first fetch of each visit starts the visit
 * (`mark_seen`), so the window runs from the visit before; refetches while
 * Now stays open leave it alone.
 */
export function useArrivalsQuery() {
  const account = useUiPrefs((s) => s.accountScope);
  const visiting = useRef(true);
  return useQuery({
    queryKey: [...ARRIVALS_KEY, "line", account ?? "all"],
    queryFn: () => {
      const markSeen = visiting.current;
      visiting.current = false;
      return fetchArrivals(account, markSeen);
    },
    // Every mount is a visit, so it always asks.
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

/** Where each email went, in request order; the daemon leaves out mail older than the ledger. */
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

/** Inbox chips for the rows on screen, keyed by message id. */
export function useArrivalModes(messageIds: readonly string[]) {
  // messageIds is stable across renders that don't change the visible
  // range (MailboxList memoizes it); without this, the list's host
  // component re-renders on every keystroke (cursor moves, stars,
  // archives), and a fresh array here would make TanStack Query re-hash
  // the key every time instead of only when the ids actually change.
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
