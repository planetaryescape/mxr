import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Schemas = components["schemas"];

/** Shapes come from the daemon's OpenAPI schema so they cannot drift. */
export type Desk = Extract<Schemas["ResponseData"], { kind: "Desk" }>;
export type DeskLane = Schemas["DeskLaneData"];
export type DeskRow = Schemas["DeskRowData"];
export type DeskLaneKind = Schemas["DeskLaneKind"];
export type DeskElsewhere = Schemas["DeskElsewhereData"];

export const DESK_LANES = [
  "owed",
  "due",
  "waiting",
  "people_new",
] as const satisfies readonly DeskLaneKind[];

/**
 * Rows fetched per lane. The desk shows a few per lane; a lane's own page
 * (`/desk?lane=…`) shows up to this many from the same query.
 */
export const DESK_LANE_LIMIT = 100;

export function deskKey(account: string | null) {
  return ["desk", account ?? "all"] as const;
}

export function fetchDesk(account: string | null): Promise<Desk> {
  const query = new URLSearchParams({ lane_limit: String(DESK_LANE_LIMIT) });
  if (account) query.set("account", account);
  return apiFetch<Desk>(`/api/v1/mail/desk?${query.toString()}`);
}

/**
 * The desk for the current account scope. The sidebar badge and the desk
 * page share this query, so a mutation's invalidation updates both.
 */
export function useDeskQuery() {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery({
    queryKey: deskKey(account),
    queryFn: () => fetchDesk(account),
    staleTime: 15_000,
    refetchInterval: 60_000,
  });
}
