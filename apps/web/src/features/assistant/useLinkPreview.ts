/*
 * What a hover card shows beyond what its chip already knows. Nothing loads
 * until the card is open (`open`), so a list of chips costs no requests. An
 * email reads `GetThreadPreviews` (no bodies); a person reads the page the
 * Messages mode already uses.
 */

import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { useUiPrefs } from "@/state/uiPrefsStore";
import type { AssistantLink } from "./links";

type Schemas = components["schemas"];
export type ThreadPreview = Schemas["ThreadPreviewData"];
export type PersonProfile = Schemas["PersonPageData"];
type PreviewsResponse = Extract<Schemas["ResponseData"], { kind: "ThreadPreviews" }>;
type PageResponse = Extract<Schemas["ResponseData"], { kind: "PersonPage" }>;

export async function fetchThreadPreview(threadId: string): Promise<ThreadPreview | null> {
  const answer = await apiFetch<PreviewsResponse>("/api/v1/mail/threads/previews", {
    method: "POST",
    body: { thread_ids: [threadId] },
  });
  return answer.previews[0] ?? null;
}

/** A person's profile with no conversation: the card shows no message text, so none is fetched. */
export async function fetchPersonProfile(
  account: string | null,
  person: string,
): Promise<PersonProfile> {
  const params = new URLSearchParams({ person, conversation: "false" });
  if (account) params.set("account", account);
  const answer = await apiFetch<PageResponse>(`/api/v1/mail/people/page?${params.toString()}`);
  return answer.page;
}

export function useLinkPreview(link: AssistantLink, open: boolean) {
  const threadId = link.kind === "thread" ? link.thread_id : null;
  const thread = useQuery({
    queryKey: ["assistant-link-preview", "thread", threadId],
    queryFn: () => fetchThreadPreview(threadId ?? ""),
    enabled: open && threadId !== null,
    staleTime: 60_000,
  });
  const account = useUiPrefs((s) => s.accountScope);
  const personId = link.kind === "person" ? link.person_id : null;
  const person = useQuery({
    queryKey: ["assistant-link-preview", "person", account ?? "all", personId],
    queryFn: () => fetchPersonProfile(account, personId ?? ""),
    enabled: open && personId !== null,
    staleTime: 60_000,
  });
  return { thread: thread.data ?? null, person: person.data ?? null };
}
