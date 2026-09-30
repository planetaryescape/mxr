/*
 * List-row gists: one line per conversation saying what it is about and
 * what it asks of you, so a row can be triaged without opening it.
 *
 * A small store outside React, keyed by thread id. Lists ask for the rows
 * on screen (`requestRowGists`); the daemon answers with the gists it has
 * cached and queues the rest, announcing each as a `ThreadGistReady` event.
 * Each row subscribes to its own thread id only (`useRowGist`), so a line
 * arriving re-renders that row and nothing else: never the list.
 */

import { useCallback, useSyncExternalStore } from "react";

import { apiFetch } from "@/api/client";
import type { DaemonEvent } from "@/api/events";
import type { components } from "@/api/generated";
import { provenanceLabel } from "@/features/thread/context/contextFormat";
import { daemonEvents } from "@/lib/ws";

type Schemas = components["schemas"];
export type ThreadGistBatch = Schemas["ThreadGistBatchData"];
type ThreadGist = Schemas["ThreadGistData"];
type BatchResponse = Extract<Schemas["ResponseData"], { kind: "ThreadGists" }>;

/** What a row shows. `ask` is the model's summary of the ask, not a quote. */
export interface RowGist {
  about: string;
  ask: string | null;
  /** "Local model gemma4 · from this thread": where the words came from. */
  source: string;
}

/** Most ids one request may carry (the daemon's cap). */
export const MAX_BATCH = 100;
/** A row asked about recently isn't asked about again until this passes. */
const REQUEST_TTL_MS = 60_000;
/** A shown gist is checked against the daemon's cache again after this. */
const GIST_TTL_MS = 5 * 60_000;
/** With no model, lists stop asking for this long. */
const NO_MODEL_RECHECK_MS = 5 * 60_000;

const gists = new Map<string, { gist: RowGist; at: number }>();
const listeners = new Map<string, Set<() => void>>();
const requestedAt = new Map<string, number>();
let noModelUntil = 0;
let unsubscribeEvents: (() => void) | undefined;

export function toRowGist(data: ThreadGist): RowGist | null {
  const about = data.gist?.trim();
  if (data.status !== "ready" || !about) return null;
  return {
    about,
    ask: data.ask?.summary?.trim() || null,
    source: data.provenance ? provenanceLabel(data.provenance, null) : "Model summary",
  };
}

function notify(threadId: string): void {
  for (const listener of listeners.get(threadId) ?? []) listener();
}

/** Store a gist from the daemon (a batch answer or an event). */
export function putGist(data: ThreadGist, now = Date.now()): void {
  const gist = toRowGist(data);
  if (!gist) return;
  gists.set(data.thread_id, { gist, at: now });
  notify(data.thread_id);
}

/** A new message changed these conversations: their gists no longer hold. */
export function forgetGists(threadIds: Iterable<string>): void {
  for (const threadId of threadIds) {
    requestedAt.delete(threadId);
    if (gists.delete(threadId)) notify(threadId);
  }
}

export function getRowGist(threadId: string): RowGist | undefined {
  return gists.get(threadId)?.gist;
}

function subscribe(threadId: string, listener: () => void): () => void {
  let set = listeners.get(threadId);
  if (!set) {
    set = new Set();
    listeners.set(threadId, set);
  }
  set.add(listener);
  return () => {
    set.delete(listener);
    if (set.size === 0) listeners.delete(threadId);
  };
}

/** The row's gist, re-rendering only this row when it arrives or goes. */
export function useRowGist(threadId: string): RowGist | undefined {
  const subscribeToRow = useCallback(
    (listener: () => void) => subscribe(threadId, listener),
    [threadId],
  );
  const read = useCallback(() => getRowGist(threadId), [threadId]);
  return useSyncExternalStore(subscribeToRow, read, read);
}

function onDaemonEvent(event: DaemonEvent): void {
  switch (event.type) {
    case "ThreadGistReady":
      if (isGistEvent(event)) putGist(event.gist);
      break;
    case "NewMessages": {
      const envelopes = (event as { envelopes?: Array<{ thread_id?: unknown }> }).envelopes ?? [];
      forgetGists(
        envelopes.flatMap((envelope) =>
          typeof envelope.thread_id === "string" ? [envelope.thread_id] : [],
        ),
      );
      break;
    }
    case "EventsLagged":
      // Missed events may include gists: let the rows ask again.
      requestedAt.clear();
      break;
  }
}

function isGistEvent(event: DaemonEvent): event is { type: string; gist: ThreadGist } {
  const gist = (event as { gist?: unknown }).gist;
  return typeof gist === "object" && gist !== null && "thread_id" in gist;
}

function listen(): void {
  unsubscribeEvents ??= daemonEvents.subscribe(onDaemonEvent);
}

export function fetchGists(threadIds: string[], generate: boolean): Promise<ThreadGistBatch> {
  return apiFetch<BatchResponse>("/api/v1/mail/gists", {
    method: "POST",
    body: { thread_ids: threadIds, generate },
  }).then((response) => response.batch);
}

/**
 * Ask for the gists of rows on screen, in the order given (visible rows
 * first). Rows already shown, or asked about a moment ago, are skipped, so
 * calling this on every scroll is cheap. When the daemon says it has no
 * model, nothing is asked for a while.
 */
export async function requestRowGists(
  threadIds: readonly string[],
  now = Date.now(),
): Promise<void> {
  if (now < noModelUntil) return;
  listen();
  const wanted: string[] = [];
  const stale = new Set<string>();
  for (const threadId of new Set(threadIds)) {
    const shown = gists.get(threadId);
    const fresh = shown && now - shown.at < GIST_TTL_MS;
    const asked = now - (requestedAt.get(threadId) ?? -Infinity) < REQUEST_TTL_MS;
    if (fresh || asked) continue;
    if (shown) stale.add(threadId);
    wanted.push(threadId);
    if (wanted.length === MAX_BATCH) break;
  }
  if (wanted.length === 0) return;
  for (const threadId of wanted) requestedAt.set(threadId, now);

  let batch: ThreadGistBatch;
  try {
    batch = await fetchGists(wanted, true);
  } catch {
    // Quiet: rows keep their snippets, and the next scroll asks again.
    for (const threadId of wanted) requestedAt.delete(threadId);
    return;
  }
  if (batch.model === "disabled") {
    noModelUntil = now + NO_MODEL_RECHECK_MS;
    return;
  }
  const answered = new Set<string>();
  for (const gist of batch.gists) {
    answered.add(gist.thread_id);
    putGist(gist, now);
  }
  // A shown gist the cache no longer has was for an older version of the
  // conversation: drop it rather than keep saying something stale.
  forgetStale([...stale].filter((threadId) => !answered.has(threadId)));
}

function forgetStale(threadIds: string[]): void {
  for (const threadId of threadIds) {
    if (gists.delete(threadId)) notify(threadId);
  }
}

/** Tests only: start from nothing. */
export function resetRowGistsForTest(): void {
  gists.clear();
  requestedAt.clear();
  listeners.clear();
  noModelUntil = 0;
  unsubscribeEvents?.();
  unsubscribeEvents = undefined;
}
