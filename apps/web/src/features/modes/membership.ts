import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

type Schemas = components["schemas"];
export type ThreadModes = Schemas["ThreadModesData"];
export type ModeMembership = Schemas["ModeMembershipData"];
export type ModeKind = Schemas["ModeKindData"];
export type ScreenerQuestion = Schemas["ScreenerQuestionData"];
type MembershipResponse = Extract<Schemas["ResponseData"], { kind: "ModeMembership" }>;

export const MEMBERSHIP_MAX_THREADS = 100;

export const MEMBERSHIP_KEY = ["mode-membership"] as const;

const MEMBERSHIP_EVIDENCE_LIMIT = 0;

export async function fetchMembership(threadId: string): Promise<ThreadModes | null> {
  const answer = await apiFetch<MembershipResponse>(
    `/api/v1/mail/modes/membership?thread_id=${encodeURIComponent(threadId)}&limit=${MEMBERSHIP_EVIDENCE_LIMIT}`,
  );
  return answer.threads[0] ?? null;
}

export async function fetchMemberships(threadIds: readonly string[]): Promise<ThreadModes[]> {
  const chunks: string[][] = [];
  for (let at = 0; at < threadIds.length; at += MEMBERSHIP_MAX_THREADS) {
    chunks.push(threadIds.slice(at, at + MEMBERSHIP_MAX_THREADS));
  }
  const answers = await Promise.all(
    chunks.map((chunk) =>
      apiFetch<MembershipResponse>("/api/v1/mail/modes/membership", {
        method: "POST",
        body: { thread_ids: chunk, limit: MEMBERSHIP_EVIDENCE_LIMIT },
      }),
    ),
  );
  return answers.flatMap((answer) => answer.threads);
}

export function useThreadModes(threadId: string | null | undefined) {
  return useQuery({
    queryKey: [...MEMBERSHIP_KEY, "thread", threadId ?? ""],
    queryFn: () => fetchMembership(threadId!),
    enabled: Boolean(threadId),
    staleTime: 15_000,
  });
}

export function useThreadModesMap(threadIds: readonly string[]) {
  const sorted = [...new Set(threadIds)].toSorted();
  return useQuery({
    queryKey: [...MEMBERSHIP_KEY, "many", sorted.join(",")],
    queryFn: async () => new Map((await fetchMemberships(sorted)).map((t) => [t.thread_id, t])),
    enabled: sorted.length > 0,
    staleTime: 15_000,
  });
}

export function otherModes(modes: ThreadModes | null | undefined, here: ModeKind | null) {
  return modes?.modes.filter((entry) => entry.mode !== here) ?? [];
}
