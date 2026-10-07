/*
 * Messages over the bridge. The daemon builds the bands, the rows, each
 * message's new text and Got it's exact words (`ListMessages`,
 * `GetPerson`, `AckMessage`); this file only moves them. Routes live under
 * /people because /messages is the per-message API.
 */

import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { getActiveQueryClient } from "@/lib/queryClient";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Schemas = components["schemas"];
export type MessagesData = Schemas["MessagesData"];
export type MessagesRow = Schemas["MessagesRowData"];
export type MessagesTopic = Schemas["MessagesTopicData"];
export type PersonPage = Schemas["PersonPageData"];
export type Conversation = Schemas["ConversationData"];
export type ConversationMessage = Schemas["ConversationMessageData"];
export type AckPlan = Schemas["AckPlanData"];
export type PersonMerge = Schemas["PersonMergeData"];
export type MergeSuggestion = Schemas["MergeSuggestionData"];
export type MessagesTurn = Schemas["MessagesTurnData"];

type ListResponse = Extract<Schemas["ResponseData"], { kind: "Messages" }>;
type PageResponse = Extract<Schemas["ResponseData"], { kind: "PersonPage" }>;
type AckResponse = Extract<Schemas["ResponseData"], { kind: "MessagesAck" }>;
type MergeResponse = Extract<Schemas["ResponseData"], { kind: "PersonMerge" }>;

/** Every Messages query starts with this, so one invalidation refreshes them. */
export const MESSAGES_KEY = ["messages-mode"] as const;

function query(params: Record<string, string | null | undefined>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value) search.set(key, value);
  }
  const text = search.toString();
  return text ? `?${text}` : "";
}

export async function fetchMessages(
  account: string | null,
  turn: MessagesTurn | null,
): Promise<MessagesData> {
  const answer = await apiFetch<ListResponse>(`/api/v1/mail/people${query({ account, turn })}`);
  return answer.messages;
}

export function useMessagesQuery(turn: MessagesTurn | null = null) {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery({
    queryKey: [...MESSAGES_KEY, "list", account ?? "all", turn ?? "all"],
    queryFn: () => fetchMessages(account, turn),
    staleTime: 15_000,
    refetchInterval: 60_000,
  });
}

export async function fetchPerson(
  account: string | null,
  person: string,
  topic: string | null,
): Promise<PersonPage> {
  const answer = await apiFetch<PageResponse>(
    `/api/v1/mail/people/page${query({ account, person, topic })}`,
  );
  return answer.page;
}

export function usePersonQuery(person: string | null, topic: string | null) {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery({
    queryKey: [...MESSAGES_KEY, "person", account ?? "all", person, topic],
    queryFn: () => fetchPerson(account, person ?? "", topic),
    enabled: person !== null,
    staleTime: 15_000,
    placeholderData: (previous, previousQuery) =>
      // Keep the page while switching topics on the same person, so the
      // header and topic list don't flash.
      previousQuery?.queryKey[3] === person ? previous : undefined,
  });
}

/** Got it: the exact text (`dryRun`), or send what was previewed. */
export async function ack(
  threadId: string,
  dryRun: boolean,
  expectText?: string,
): Promise<AckPlan> {
  const answer = await apiFetch<AckResponse>("/api/v1/mail/people/ack", {
    method: "POST",
    body: {
      thread_id: threadId,
      dry_run: dryRun,
      ...(expectText !== undefined ? { expect_text: expectText } : {}),
    },
  });
  return answer.ack;
}

export async function mergePeople(
  accountId: string,
  into: string,
  addresses: string[],
  dryRun: boolean,
): Promise<PersonMerge> {
  const answer = await apiFetch<MergeResponse>("/api/v1/mail/people/merge", {
    method: "POST",
    body: { account_id: accountId, into, addresses, dry_run: dryRun },
  });
  return answer.merge;
}

/** Pin or unpin a person: Messages' pins are the cadence watchlist. */
export async function setPinned(accountId: string, email: string, pinned: boolean): Promise<void> {
  await apiFetch(`/api/v1/platform/cadence/${pinned ? "watch" : "unwatch"}`, {
    method: "POST",
    body: { account_id: accountId, email },
  });
}

export async function refreshMessages(): Promise<void> {
  await getActiveQueryClient()
    ?.invalidateQueries({ queryKey: MESSAGES_KEY })
    .catch(() => undefined);
}
