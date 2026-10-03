/*
 * The rail over the bridge (`GetRail`): Now, the five modes and Inbox in
 * order with their keys, counts and early-version notes, plus the pages
 * under More. The sidebar and the phone tabs read it, so both follow the
 * daemon's order and words.
 */

import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Schemas = components["schemas"];
export type Rail = Schemas["RailData"];
export type RailEntry = Schemas["RailEntryData"];
export type RailLink = Schemas["RailLinkData"];
type RailResponse = Extract<Schemas["ResponseData"], { kind: "Rail" }>;

/** Where each rail entry opens in the web app. */
export const RAIL_PATHS: Record<string, string> = {
  now: "/now",
  messages: "/messages",
  todo: "/todo",
  updates: "/updates",
  reading: "/reading",
  archive: "/archive",
  inbox: "/m/inbox",
};

export async function fetchRail(account: string | null): Promise<Rail> {
  const query = account ? `?account=${encodeURIComponent(account)}` : "";
  const answer = await apiFetch<RailResponse>(`/api/v1/mail/rail${query}`);
  return answer.rail;
}

export function useRailQuery() {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery({
    queryKey: ["rail", account ?? "all"],
    queryFn: () => fetchRail(account),
    staleTime: 15_000,
    refetchInterval: 60_000,
  });
}

/**
 * The number a rail entry shows. Now's badge counts work; To do and
 * Messages show a quiet count of what they hold. Updates and Reading show
 * none, since nothing there is owed (blueprint 22, the rail).
 */
export function railCount(entry: RailEntry): { value: number; badge: boolean } | null {
  if (entry.badge != null && entry.badge > 0) return { value: entry.badge, badge: true };
  if ((entry.id === "todo" || entry.id === "messages") && entry.count != null && entry.count > 0) {
    return { value: entry.count, badge: false };
  }
  return null;
}
