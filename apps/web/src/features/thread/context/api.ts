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
export type ThreadPromise = Schemas["ThreadPromiseData"];

export type ThreadGistBatch = Schemas["ThreadGistBatchData"];
export type GistModel = Schemas["GistModelData"];

type ContextResponse = Extract<Schemas["ResponseData"], { kind: "ThreadContext" }>;
type GistBatchResponse = Extract<Schemas["ResponseData"], { kind: "ThreadGists" }>;
type GistResponse = Extract<Schemas["ResponseData"], { kind: "ThreadGist" }>;

/*
 * Keyed under ["thread", id] so every invalidation of the thread (a sent
 * reply, a mutation) refreshes its context too.
 */
export const threadContextKey = (threadId: string) => ["thread", threadId, "context"] as const;
export const threadGistKey = (threadId: string, policy: string) =>
  ["thread", threadId, "gist", policy] as const;

/** Every cached gist, for invalidation when LLM settings change. */
export const isGistQuery = (query: { queryKey: readonly unknown[] }) =>
  query.queryKey[0] === "thread" && query.queryKey[2] === "gist";

/**
 * Query options shared by the pane (which waits for it) and the reader. No
 * retries, on failure or on mount: the pane holds the thread back while this
 * is pending, so a retry when the reader mounts would put the pane back in
 * its loading state and unmount the reader again. A thread without its
 * facts beats one held up by backoff.
 */
export function threadContextQuery(threadId: string) {
  return {
    queryKey: threadContextKey(threadId),
    queryFn: () => fetchThreadContext(threadId),
    staleTime: 30_000,
    retry: false,
    retryOnMount: false,
  };
}

/** The gist: cached by the daemon per newest message, so no retries here. */
export function threadGistQuery(threadId: string, policy: string) {
  return {
    queryKey: threadGistKey(threadId, policy),
    queryFn: () => fetchThreadGist(threadId),
    staleTime: 5 * 60_000,
    retry: false,
  };
}

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

/**
 * List-row gists for many conversations: cached ones at once; with
 * `generate`, the missing ones from people are queued and arrive as
 * `ThreadGistReady` events. At most 100 ids (the daemon's
 * `THREAD_GISTS_MAX_BATCH`).
 */
export async function fetchThreadGists(
  threadIds: string[],
  generate: boolean,
): Promise<ThreadGistBatch> {
  const response = await apiFetch<GistBatchResponse>("/api/v1/mail/gists", {
    method: "POST",
    body: { thread_ids: threadIds, generate },
  });
  return response.batch;
}
