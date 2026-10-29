/*
 * Which modes hold a thread, and why (`GetModeMembership`). The reader's
 * "Also in" line and the rows' lines read it; the daemon writes every line,
 * so each client names the other modes the same way.
 */

import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { threadAccounts, conversationKey, type ThreadRef } from "@/features/mailbox/rowKey";

type Schemas = components["schemas"];
export type ThreadModes = Schemas["ThreadModesData"];
export type ModeMembership = Schemas["ModeMembershipData"];
export type ModeKind = Schemas["ModeKindData"];
export type ScreenerQuestion = Schemas["ScreenerQuestionData"];
type MembershipResponse = Extract<Schemas["ResponseData"], { kind: "ModeMembership" }>;

/** The most threads one membership request may name (the daemon's cap). */
export const MEMBERSHIP_MAX_THREADS = 100;

export const MEMBERSHIP_KEY = ["mode-membership"] as const;

export async function fetchMembership(
  threadId: string,
  accountId?: string | null,
): Promise<ThreadModes | null> {
  const query = new URLSearchParams({ thread_id: threadId });
  // A thread id can be held by two accounts: ask about the row's own.
  if (accountId) query.set("account_id", accountId);
  const answer = await apiFetch<MembershipResponse>(`/api/v1/mail/modes/membership?${query}`);
  return answer.threads[0] ?? null;
}

/** Many threads at once, at most 100 per request. Accounts ride along when the rows name them. */
export async function fetchMemberships(refs: readonly ThreadRef[]): Promise<ThreadModes[]> {
  const chunks: ThreadRef[][] = [];
  for (let at = 0; at < refs.length; at += MEMBERSHIP_MAX_THREADS) {
    chunks.push(refs.slice(at, at + MEMBERSHIP_MAX_THREADS));
  }
  const answers = await Promise.all(
    chunks.map((chunk) => {
      const accounts = threadAccounts(chunk);
      return apiFetch<MembershipResponse>("/api/v1/mail/modes/membership", {
        method: "POST",
        body: {
          thread_ids: [...new Set(chunk.map((ref) => ref.thread_id))],
          ...(accounts.length > 0 ? { thread_accounts: accounts } : {}),
        },
      });
    }),
  );
  return answers.flatMap((answer) => answer.threads);
}

export function useThreadModes(threadId: string | null | undefined, accountId?: string | null) {
  return useQuery({
    queryKey: [...MEMBERSHIP_KEY, "thread", accountId ?? "", threadId ?? ""],
    queryFn: () => fetchMembership(threadId!, accountId),
    enabled: Boolean(threadId),
    staleTime: 15_000,
  });
}

/** Membership by `conversationKey`; a bare thread id also resolves, to the first account's entry. */
export type ModesByThread = Map<string, ThreadModes>;

export function modesByThread(threads: readonly ThreadModes[]): ModesByThread {
  const map: ModesByThread = new Map();
  for (const entry of threads) {
    map.set(conversationKey(entry.thread_id, entry.account_id), entry);
    const bare = entry.thread_id;
    if (!map.has(bare)) map.set(bare, entry);
  }
  return map;
}

/**
 * Membership for a list's threads, keyed by `conversationKey`. A bare thread id
 * is for data that has no account.
 */
export function useThreadModesMap(threads: readonly (string | ThreadRef)[]) {
  const refs = threads.map((t) => (typeof t === "string" ? { thread_id: t } : t));
  const sorted = [
    ...new Map(refs.map((ref) => [conversationKey(ref.thread_id, ref.account_id), ref])).entries(),
  ].toSorted(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  return useQuery({
    queryKey: [...MEMBERSHIP_KEY, "many", sorted.map(([key]) => key).join(",")],
    queryFn: async () => modesByThread(await fetchMemberships(sorted.map(([, ref]) => ref))),
    enabled: sorted.length > 0,
    staleTime: 15_000,
  });
}

/** The modes holding a thread other than the one on screen. */
export function otherModes(modes: ThreadModes | null | undefined, here: ModeKind | null) {
  return modes?.modes.filter((entry) => entry.mode !== here) ?? [];
}
