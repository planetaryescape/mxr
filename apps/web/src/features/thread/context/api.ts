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

export const threadContextKey = (threadId: string) => ["thread", threadId, "context"] as const;
export const threadGistKey = (threadId: string, policy: string) =>
  ["thread", threadId, "gist", policy] as const;

export const isGistQuery = (query: { queryKey: readonly unknown[] }) =>
  query.queryKey[0] === "thread" && query.queryKey[2] === "gist";

export function threadContextQuery(threadId: string) {
  return {
    queryKey: threadContextKey(threadId),
    queryFn: () => fetchThreadContext(threadId),
    staleTime: 30_000,
    retry: false,
    retryOnMount: false,
  };
}

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

export async function fetchThreadGists(
  threadIds: string[],
  generate: boolean,
  automated = false,
): Promise<ThreadGistBatch> {
  const response = await apiFetch<GistBatchResponse>("/api/v1/mail/gists", {
    method: "POST",
    body: { thread_ids: threadIds, generate, ...(automated ? { automated } : {}) },
  });
  return response.batch;
}
