/*
 * What a hover card shows beyond what its chip already knows. Nothing loads
 * until the card is open (`open`), so a list of chips costs no requests. An
 * email reads `GetThreadPreviews` (no bodies); a person reads the page the
 * Messages mode already uses.
 */

import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { usePersonQuery } from "@/features/messages/api";
import type { AssistantLink } from "./links";

type Schemas = components["schemas"];
export type ThreadPreview = Schemas["ThreadPreviewData"];
type PreviewsResponse = Extract<Schemas["ResponseData"], { kind: "ThreadPreviews" }>;

export async function fetchThreadPreview(threadId: string): Promise<ThreadPreview | null> {
  const answer = await apiFetch<PreviewsResponse>("/api/v1/mail/threads/previews", {
    method: "POST",
    body: { thread_ids: [threadId] },
  });
  return answer.previews[0] ?? null;
}

export function useLinkPreview(link: AssistantLink, open: boolean) {
  const threadId = link.kind === "thread" ? link.thread_id : null;
  const thread = useQuery({
    queryKey: ["assistant-link-preview", "thread", threadId],
    queryFn: () => fetchThreadPreview(threadId ?? ""),
    enabled: open && threadId !== null,
    staleTime: 60_000,
  });
  const person = usePersonQuery(open && link.kind === "person" ? link.person_id : null, null);
  return { thread: thread.data ?? null, person: person.data ?? null };
}
