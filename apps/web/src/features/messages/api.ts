import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { getActiveQueryClient } from "@/lib/queryClient";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { NO_FILTER, type MessagesFilter } from "./filters";

type Schemas = components["schemas"];
export type MessagesData = Schemas["MessagesData"];
export type MessagesRow = Schemas["MessagesRowData"];
export type MessagesRowDraft = Schemas["MessagesRowDraftData"];
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

export const MESSAGES_KEY = ["messages-mode"] as const;

export const LOADING_MESSAGES: MessagesData = {
  generated_at: "",
  header: "",
  your_turn: [],
  pinned: [],
  recent: [],
  quiet: [],
  recent_total: 0,
  quiet_total: 0,
  row_count: 0,
  thread_count: 0,
  merge_suggestion_count: 0,
};

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
  filter: MessagesFilter,
  signal?: AbortSignal,
): Promise<MessagesData> {
  const answer = await apiFetch<ListResponse>(
    `/api/v1/mail/people${query({
      account,
      turn: filter.turn,
      unread: filter.unread ? "true" : null,
      pace: filter.pastPace ? "true" : null,
    })}`,
    { signal },
  );
  return answer.messages;
}

const LIST_ACCOUNT_AT = MESSAGES_KEY.length + 1;

export function useMessagesQuery(filter: MessagesFilter = NO_FILTER) {
  const account = useUiPrefs((s) => s.accountScope);
  const scope = account ?? "all";
  return useQuery({
    queryKey: [
      ...MESSAGES_KEY,
      "list",
      scope,
      filter.turn ?? "attention",
      ...(filter.unread ? ["unread"] : []),
      ...(filter.pastPace ? ["pace"] : []),
    ],
    queryFn: ({ signal }) => fetchMessages(account, filter, signal),
    placeholderData: (previous, previousQuery) => {
      if (previousQuery?.queryKey[LIST_ACCOUNT_AT] !== scope) return undefined;
      return previousQuery.queryKey[LIST_ACCOUNT_AT + 1] === (filter.turn ?? "attention")
        ? previous
        : LOADING_MESSAGES;
    },
    staleTime: 15_000,
    refetchInterval: 60_000,
  });
}

export async function fetchPerson(
  account: string | null,
  person: string,
  topic: string | null,
  signal?: AbortSignal,
): Promise<PersonPage> {
  const answer = await apiFetch<PageResponse>(
    `/api/v1/mail/people/page${query({ account, person, topic })}`,
    { signal },
  );
  return answer.page;
}

const personKey = (account: string | null, person: string | null, topic: string | null) =>
  [...MESSAGES_KEY, "person", account ?? "all", person, topic] as const;

export function rememberTopicPage(person: string, topic: string, page: PersonPage): void {
  const account = useUiPrefs.getState().accountScope;
  getActiveQueryClient()?.setQueryData(personKey(account, person, topic), page);
}

export function usePersonQuery(person: string | null, topic: string | null) {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery({
    queryKey: personKey(account, person, topic),
    queryFn: ({ signal }) => fetchPerson(account, person ?? "", topic, signal),
    enabled: person !== null,
    staleTime: 15_000,
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[3] === person ? previous : undefined,
  });
}

export async function previewAck(threadId: string, signal?: AbortSignal): Promise<AckPlan> {
  const answer = await apiFetch<AckResponse>("/api/v1/mail/people/ack", {
    method: "POST",
    body: { thread_id: threadId, dry_run: true },
    signal,
  });
  return answer.ack;
}

export async function sendAck(plan: AckPlan): Promise<AckPlan> {
  const answer = await apiFetch<AckResponse>("/api/v1/mail/people/ack", {
    method: "POST",
    body: {
      thread_id: plan.thread_id,
      dry_run: false,
      expect_text: plan.text,
      preview_token: plan.preview_token,
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
