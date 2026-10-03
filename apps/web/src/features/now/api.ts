/*
 * Now over the bridge (`GetNow`). The daemon caps every section and writes
 * every line, so the web, the TUI and `mxr now` show the same ten things;
 * this file only moves them.
 */

import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Schemas = components["schemas"];
export type Now = Schemas["NowData"];
export type NowPerson = Schemas["NowPersonData"];
export type NowTodo = Schemas["NowTodoData"];
export type NowUpdatesCard = Schemas["NowUpdatesCardData"];
export type NowReadingPick = Schemas["NowReadingPickData"];
type NowResponse = Extract<Schemas["ResponseData"], { kind: "Now" }>;

export const NOW_KEY = ["now"] as const;

export async function fetchNow(account: string | null): Promise<Now> {
  const query = account ? `?account=${encodeURIComponent(account)}` : "";
  const answer = await apiFetch<NowResponse>(`/api/v1/mail/now${query}`);
  return answer.now;
}

export function useNowQuery() {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery({
    queryKey: [...NOW_KEY, account ?? "all"],
    queryFn: () => fetchNow(account),
    staleTime: 15_000,
    refetchInterval: 60_000,
  });
}
