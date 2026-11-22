import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Schemas = components["schemas"];
export type Rail = Schemas["RailData"];
export type RailEntry = Schemas["RailEntryData"];
export type RailLink = Schemas["RailLinkData"];
type RailResponse = Extract<Schemas["ResponseData"], { kind: "Rail" }>;

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

export function railCount(entry: RailEntry): { value: number; badge: boolean } | null {
  if (entry.badge != null && entry.badge > 0) return { value: entry.badge, badge: true };
  if ((entry.id === "todo" || entry.id === "messages") && entry.count != null && entry.count > 0) {
    return { value: entry.count, badge: false };
  }
  return null;
}
