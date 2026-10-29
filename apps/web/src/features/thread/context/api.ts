/*
 * The reader's context block comes from two daemon requests: facts from the
 * store (fast, no model) and the model's gist and ask (slow, cached per
 * thread and newest message). Types come from the generated protocol schema
 * so they can't drift from the Rust ones.
 */

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { threadAccounts, type ThreadRef } from "@/features/mailbox/rowKey";

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
export const threadContextKey = (threadId: string, accountId?: string | null) =>
  ["thread", threadId, "context", accountId ?? null] as const;
export const threadGistKey = (threadId: string, policy: string, accountId?: string | null) =>
  ["thread", threadId, "gist", policy, accountId ?? null] as const;

/** `?account_id=` for a thread id that two accounts can hold; empty without an account. */
function accountQuery(accountId?: string | null, extra: Record<string, string> = {}): string {
  const query = new URLSearchParams(extra);
  if (accountId) query.set("account_id", accountId);
  const text = query.toString();
  return text ? `?${text}` : "";
}

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
export function threadContextQuery(threadId: string, accountId?: string | null) {
  return {
    queryKey: threadContextKey(threadId, accountId),
    queryFn: () => fetchThreadContext(threadId, accountId),
    staleTime: 30_000,
    retry: false,
    retryOnMount: false,
  };
}

/** The gist: cached by the daemon per newest message, so no retries here. */
export function threadGistQuery(threadId: string, policy: string, accountId?: string | null) {
  return {
    queryKey: threadGistKey(threadId, policy, accountId),
    queryFn: () => fetchThreadGist(threadId, false, accountId),
    staleTime: 5 * 60_000,
    retry: false,
  };
}

export async function fetchThreadContext(
  threadId: string,
  accountId?: string | null,
): Promise<ThreadContext> {
  const response = await apiFetch<ContextResponse>(
    `/api/v1/mail/threads/${encodeURIComponent(threadId)}/context${accountQuery(accountId)}`,
  );
  return response.context;
}

export async function fetchThreadGist(
  threadId: string,
  refresh = false,
  accountId?: string | null,
): Promise<ThreadGist> {
  const query = accountQuery(accountId, refresh ? { refresh: "true" } : {});
  const response = await apiFetch<GistResponse>(
    `/api/v1/mail/threads/${encodeURIComponent(threadId)}/context/gist${query}`,
  );
  return response.gist;
}

/**
 * List-row gists for many conversations: cached ones at once; with
 * `generate`, the missing ones from people (and, with `automated`, from
 * automated senders: Updates) are queued and arrive as `ThreadGistReady`
 * events. At most 100 ids (the daemon's `THREAD_GISTS_MAX_BATCH`).
 */
export async function fetchThreadGists(
  threads: readonly ThreadRef[],
  generate: boolean,
  automated = false,
): Promise<ThreadGistBatch> {
  const response = await apiFetch<GistBatchResponse>("/api/v1/mail/gists", {
    method: "POST",
    // Omitted when false: the request every list sends today stays as it was.
    body: {
      thread_ids: [...new Set(threads.map((thread) => thread.thread_id))],
      ...(threadAccounts(threads).length > 0 ? { thread_accounts: threadAccounts(threads) } : {}),
      generate,
      ...(automated ? { automated } : {}),
    },
  });
  return response.batch;
}
