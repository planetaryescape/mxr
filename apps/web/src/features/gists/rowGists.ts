/*
 * List-row gists: one line per conversation saying what it is about and
 * what it asks of you, so a row can be triaged without opening it.
 *
 * A small store outside React, keyed by thread id. Lists ask for the rows
 * on screen (`requestRowGists`); the daemon answers with the gists it has
 * cached for each conversation as it is now, and queues the rest,
 * announcing each as a `ThreadGistReady` event (never one that went stale
 * while it was written). Each row subscribes to its own thread id only
 * (`useRowGist`), so a line arriving re-renders that row and nothing else.
 *
 * Freshness is one epoch: every invalidation bumps it, and an answer to a
 * request sent in an older epoch is dropped, so a slow response can't put
 * back what an invalidation took away. Lists watch the epoch and ask again.
 */

import { useCallback, useSyncExternalStore } from "react";

import type { DaemonEvent } from "@/api/events";
import {
  fetchThreadGists,
  type GistModel,
  type ThreadGist,
  type ThreadGistBatch,
} from "@/features/thread/context/api";
import { provenanceLabel } from "@/features/thread/context/contextFormat";
import { daemonEvents } from "@/lib/ws";

/** What a row shows. `ask` is the model's summary of the ask, not a quote. */
export interface RowGist {
  about: string;
  ask: string | null;
  /** "Local model gemma4 · from this thread": where the words came from. */
  source: string;
}

/** Most ids one request may carry (the daemon's `THREAD_GISTS_MAX_BATCH`). */
export const MAX_BATCH = 100;
/** Most rows remembered, shown or asked about; the oldest go first. */
export const STATE_CAP = 500;
/** A row asked about recently isn't asked about again until this passes. */
const REQUEST_TTL_MS = 60_000;
/** With no model, lists stop asking for this long. */
const NO_MODEL_RECHECK_MS = 5 * 60_000;

/** Insertion-ordered maps, so the first key is the oldest. */
const gists = new Map<string, RowGist>();
const requestedAt = new Map<string, number>();
const listeners = new Map<string, Set<() => void>>();
let epoch = 0;
const epochListeners = new Set<() => void>();
let noModelUntil = 0;
/** What the daemon last said about its model; `undefined` until it has. */
let model: GistModel | undefined;
const modelListeners = new Set<() => void>();
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

/** Set as the newest entry, dropping the oldest past the cap. */
function remember<T>(map: Map<string, T>, key: string, value: T): string[] {
  map.delete(key);
  map.set(key, value);
  const evicted: string[] = [];
  while (map.size > STATE_CAP) {
    const oldest = map.keys().next().value;
    if (oldest === undefined) break;
    map.delete(oldest);
    evicted.push(oldest);
  }
  return evicted;
}

/** Store a gist from the daemon (a batch answer or an event). */
export function putGist(data: ThreadGist): void {
  const gist = toRowGist(data);
  if (!gist) return;
  const shown = gists.get(data.thread_id);
  const same =
    shown?.about === gist.about && shown.ask === gist.ask && shown.source === gist.source;
  for (const evicted of remember(gists, data.thread_id, same ? shown : gist)) notify(evicted);
  // Same words: nothing to redraw.
  if (!same) notify(data.thread_id);
}

function bumpEpoch(): void {
  epoch += 1;
  for (const listener of epochListeners) listener();
}

/** New messages changed these conversations: drop their lines; ask again. */
export function forgetGists(threadIds: Iterable<string>): void {
  for (const threadId of threadIds) {
    requestedAt.delete(threadId);
    if (gists.delete(threadId)) notify(threadId);
  }
  bumpEpoch();
}

/**
 * Nothing shown can be trusted (missed or sampled events, a reconnect):
 * drop every line and let the lists on screen ask again.
 */
export function invalidateRowGists(): void {
  requestedAt.clear();
  const shown = [...gists.keys()];
  gists.clear();
  for (const threadId of shown) notify(threadId);
  bumpEpoch();
}

function subscribeEpoch(listener: () => void): () => void {
  epochListeners.add(listener);
  return () => epochListeners.delete(listener);
}

/** Changes on every invalidation; lists re-ask for their rows. */
export function useGistEpoch(): number {
  return useSyncExternalStore(
    subscribeEpoch,
    () => epoch,
    () => epoch,
  );
}

function setModel(next: GistModel): void {
  if (model === next) return;
  model = next;
  for (const listener of modelListeners) listener();
}

function subscribeModel(listener: () => void): () => void {
  modelListeners.add(listener);
  return () => modelListeners.delete(listener);
}

/** The daemon's last word on its model, for reserving row space. */
export function useGistModel(): GistModel | undefined {
  return useSyncExternalStore(
    subscribeModel,
    () => model,
    () => model,
  );
}

export function getRowGist(threadId: string): RowGist | undefined {
  return gists.get(threadId);
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
      // The daemon never announces a gist that went stale while written.
      if (isGistEvent(event)) putGist(event.gist);
      break;
    case "NewMessages": {
      const { envelopes = [], total = 0 } = event as {
        envelopes?: Array<{ thread_id?: unknown }>;
        total?: number;
      };
      // A capped event is only a sample: any conversation may have changed.
      if (total > envelopes.length) {
        invalidateRowGists();
        break;
      }
      forgetGists(
        envelopes.flatMap((envelope) =>
          typeof envelope.thread_id === "string" ? [envelope.thread_id] : [],
        ),
      );
      break;
    }
    case "EventsLagged":
      // A missed NewMessages could leave a stale line up.
      invalidateRowGists();
      break;
  }
}

/**
 * The event stream is typed loosely (any `type` string passes), so check
 * the fields `putGist` reads before trusting the payload.
 */
function isGistEvent(event: DaemonEvent): event is { type: string; gist: ThreadGist } {
  const gist = (event as { gist?: unknown }).gist;
  if (typeof gist !== "object" || gist === null) return false;
  const { thread_id, status, gist: text } = gist as Record<string, unknown>;
  return (
    typeof thread_id === "string" &&
    typeof status === "string" &&
    (text == null || typeof text === "string")
  );
}

function listen(): void {
  if (unsubscribeEvents) return;
  const offEvents = daemonEvents.subscribe(onDaemonEvent);
  // Events sent while the stream was down are lost: on reconnecting, what
  // is shown may be stale.
  let last = daemonEvents.getStatus().state;
  const offStatus = daemonEvents.onStatus(({ state }) => {
    if (state === "connected" && last !== "connected") invalidateRowGists();
    last = state;
  });
  unsubscribeEvents = () => {
    offEvents();
    offStatus();
  };
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
  for (const threadId of new Set(threadIds)) {
    const asked = now - (requestedAt.get(threadId) ?? -Infinity) < REQUEST_TTL_MS;
    if (gists.has(threadId) || asked) continue;
    wanted.push(threadId);
    if (wanted.length === MAX_BATCH) break;
  }
  if (wanted.length === 0) return;
  for (const threadId of wanted) remember(requestedAt, threadId, now);
  const sentIn = epoch;

  let batch: ThreadGistBatch;
  try {
    batch = await fetchThreadGists(wanted, true);
  } catch {
    // Quiet: rows keep their snippets, and the next scroll asks again.
    for (const threadId of wanted) requestedAt.delete(threadId);
    return;
  }
  if (sentIn !== epoch) {
    // Invalidated while the request was out: this answer may be stale.
    // The list asks again for what is still on screen.
    for (const threadId of wanted) requestedAt.delete(threadId);
    return;
  }
  setModel(batch.model);
  if (batch.model === "disabled") {
    noModelUntil = now + NO_MODEL_RECHECK_MS;
    return;
  }
  for (const gist of batch.gists) putGist(gist);
}

/** Tests only: start from nothing. */
export function resetRowGistsForTest(): void {
  gists.clear();
  requestedAt.clear();
  listeners.clear();
  noModelUntil = 0;
  model = undefined;
  modelListeners.clear();
  epoch = 0;
  epochListeners.clear();
  unsubscribeEvents?.();
  unsubscribeEvents = undefined;
}
