/*
 * The reader's context block comes from two daemon requests: facts from the
 * store (fast, no model) and the model's gist and ask (slow, cached per
 * thread and newest message). Types come from the generated protocol schema
 * so they can't drift from the Rust ones.
 */

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

type Schemas = components["schemas"];
export type ThreadContext = Schemas["ThreadContextData"];
export type ThreadCounterparty = Schemas["ThreadCounterpartyData"];
export type ThreadGist = Schemas["ThreadGistData"];
export type ThreadAsk = Schemas["ThreadAskData"];
export type AiProvenance = Schemas["AiProvenanceData"];
export type Commitment = Schemas["CommitmentData"];

type ContextResponse = Extract<Schemas["ResponseData"], { kind: "ThreadContext" }>;
type GistResponse = Extract<Schemas["ResponseData"], { kind: "ThreadGist" }>;

/*
 * Keyed under ["thread", id] so every invalidation of the thread (a sent
 * reply, a mutation) refreshes its context too.
 */
export const threadContextKey = (threadId: string) => ["thread", threadId, "context"] as const;
export const threadGistKey = (threadId: string) => ["thread", threadId, "gist"] as const;

export async function fetchThreadContext(threadId: string): Promise<ThreadContext> {
  const response = await apiFetch<ContextResponse>(
    `/api/v1/mail/threads/${encodeURIComponent(threadId)}/context`,
  );
  return response.context;
}

export async function fetchThreadGist(threadId: string, refresh = false): Promise<ThreadGist> {
  const query = refresh ? "?refresh=true" : "";
  const response = await apiFetch<GistResponse>(
    `/api/v1/mail/threads/${encodeURIComponent(threadId)}/context/gist${query}`,
  );
  return response.gist;
}
