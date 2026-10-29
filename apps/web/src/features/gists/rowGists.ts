/*
 * List-row gists: one line per conversation saying what it is about and
 * what it asks of you, so a row can be triaged without opening it.
 *
 * A small store outside React, keyed by the conversation (`conversationKey`:
 * thread id within its account, since two accounts can hold one thread id;
 * data without an account uses the bare thread id). Lists ask for the rows
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
  type AiProvenance,
  type GistModel,
  type ThreadGist,
  type ThreadGistBatch,
} from "@/features/thread/context/api";
import { conversationKey, type ThreadRef } from "@/features/mailbox/rowKey";
import { provenanceLabel } from "@/features/thread/context/contextFormat";
import { isDaemonDown } from "@/lib/daemonAvailability";
import { daemonEvents } from "@/lib/ws";

/** What a row shows. `ask` is the model's summary of the ask, not a quote. */
export interface RowGist {
  about: string;
  ask: string | null;
  /** "Local model gemma4 · from this thread": where the words came from. */
  source: string;
  /** The raw provenance, for lists that word the source in their own nouns. */
  provenance: AiProvenance | null;
  /** The message the gist answers for: lists show it only for that message. */
  newest: string | null;
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
/**
 * When each row was last asked about, per kind of ask: a conversation list's
 * request skips automated mail, so Updates asking for the same thread a
 * moment later is a different question.
 */
const requestedAt = new Map<string, number>();
const askedKey = (key: string, automated: boolean) =>
  `${automated ? "automated" : "people"}:${key}`;
function forgetAsked(key: string): void {
  requestedAt.delete(askedKey(key, false));
  requestedAt.delete(askedKey(key, true));
}
/** Whether a conversation key (bare, or `thread-<account>:<thread>`) is for this thread id. */
const isThreadKey = (key: string, threadId: string) =>
  key === threadId || key.endsWith(`:${threadId}`);
const asRef = (thread: string | ThreadRef): ThreadRef =>
  typeof thread === "string" ? { thread_id: thread } : thread;
/** Keys a conversation may be stored or watched under: its own, and the bare one. */
const keysOf = (ref: ThreadRef): string[] =>
  ref.account_id
    ? [conversationKey(ref.thread_id, ref.account_id), ref.thread_id]
    : [ref.thread_id];
const listeners = new Map<string, Set<() => void>>();
let epoch = 0;
const epochListeners = new Set<() => void>();
let noModelUntil = 0;
/** What the daemon last said about its model; `undefined` until it has. */
let model: GistModel | undefined;
const modelListeners = new Set<() => void>();
let unsubscribeEvents: (() => void) | undefined;

/** Model text and daemon payloads are typed loosely at runtime; only strings are words. */
const words = (value: unknown): string => (typeof value === "string" ? value.trim() : "");

export function toRowGist(data: ThreadGist): RowGist | null {
  const about = words(data.gist);
  if (data.status !== "ready" || !about) return null;
  return {
    about,
    ask: words(data.ask?.summary) || null,
    source: data.provenance ? provenanceLabel(data.provenance, null) : "Model summary",
    provenance: data.provenance ?? null,
    newest: words(data.newest_message_id) || null,
  };
}

function notify(key: string): void {
  for (const listener of listeners.get(key) ?? []) listener();
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
  // A gist the daemon could not tie to an account is stored bare.
  const key = conversationKey(data.thread_id, (data as { account_id?: string | null }).account_id);
  const shown = gists.get(key);
  const same =
    shown?.about === gist.about &&
    shown.ask === gist.ask &&
    shown.source === gist.source &&
    shown.newest === gist.newest;
  for (const evicted of remember(gists, key, same ? shown : gist)) {
    notify(evicted);
  }
  // Same words: nothing to redraw.
  if (!same) notify(key);
}

function bumpEpoch(): void {
  epoch += 1;
  for (const listener of epochListeners) listener();
}

/** New messages changed these conversations: drop their lines; ask again. */
export function forgetGists(threads: Iterable<string | ThreadRef>): void {
  for (const thread of threads) {
    const ref = asRef(thread);
    // An id without an account forgets every account's copy of the thread.
    const keys = ref.account_id
      ? keysOf(ref)
      : [
          ...new Set(
            [
              ...gists.keys(),
              ...[...requestedAt.keys()].map((asked) => asked.split(":").slice(1).join(":")),
            ].filter((key) => isThreadKey(key, ref.thread_id)),
          ),
          ref.thread_id,
        ];
    for (const key of keys) {
      forgetAsked(key);
      if (gists.delete(key)) notify(key);
    }
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
  for (const key of shown) notify(key);
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

/** A row's gist: its own account's, or the bare one when the daemon named no account. */
export function getRowGist(threadId: string, accountId?: string | null): RowGist | undefined {
  return gists.get(conversationKey(threadId, accountId)) ?? gists.get(threadId);
}

function subscribe(keys: readonly string[], listener: () => void): () => void {
  for (const key of keys) {
    let set = listeners.get(key);
    if (!set) {
      set = new Set();
      listeners.set(key, set);
    }
    set.add(listener);
  }
  return () => {
    for (const key of keys) {
      const set = listeners.get(key);
      set?.delete(listener);
      if (set?.size === 0) listeners.delete(key);
    }
  };
}

/** The row's gist, re-rendering only this row when it arrives or goes. */
export function useRowGist(threadId: string, accountId?: string | null): RowGist | undefined {
  const subscribeToRow = useCallback(
    (listener: () => void) =>
      subscribe(keysOf({ thread_id: threadId, account_id: accountId }), listener),
    [threadId, accountId],
  );
  const read = useCallback(() => getRowGist(threadId, accountId), [threadId, accountId]);
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
        envelopes?: Array<{ thread_id?: unknown; account_id?: unknown }>;
        total?: number;
      };
      // A capped event is only a sample: any conversation may have changed.
      if (total > envelopes.length) {
        invalidateRowGists();
        break;
      }
      forgetGists(
        envelopes.flatMap((envelope) =>
          typeof envelope.thread_id === "string"
            ? [
                {
                  thread_id: envelope.thread_id,
                  account_id: typeof envelope.account_id === "string" ? envelope.account_id : null,
                },
              ]
            : [],
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
 * model, nothing is asked for a while. `automated` also asks for automated
 * senders' mail (receipts, alerts), which Updates shows.
 */
export async function requestRowGists(
  threads: readonly (string | ThreadRef)[],
  now = Date.now(),
  { automated = false }: { automated?: boolean } = {},
): Promise<void> {
  if (now < noModelUntil) return;
  listen();
  // Nothing to ask while the daemon is down; reconnecting re-asks.
  if (isDaemonDown()) return;
  const wanted: { ref: ThreadRef; key: string }[] = [];
  const seen = new Set<string>();
  for (const thread of threads) {
    const ref = asRef(thread);
    const key = conversationKey(ref.thread_id, ref.account_id);
    if (seen.has(key)) continue;
    seen.add(key);
    const asked = now - (requestedAt.get(askedKey(key, automated)) ?? -Infinity) < REQUEST_TTL_MS;
    if (getRowGist(ref.thread_id, ref.account_id) || asked) continue;
    wanted.push({ ref, key });
    if (wanted.length === MAX_BATCH) break;
  }
  if (wanted.length === 0) return;
  for (const { key } of wanted) remember(requestedAt, askedKey(key, automated), now);
  const sentIn = epoch;

  let batch: ThreadGistBatch;
  try {
    batch = await fetchThreadGists(
      wanted.map(({ ref }) => ref),
      true,
      automated,
    );
  } catch {
    // Quiet: rows keep their snippets, and the next scroll asks again.
    for (const { key } of wanted) requestedAt.delete(askedKey(key, automated));
    return;
  }
  if (sentIn !== epoch) {
    // Invalidated while the request was out: this answer may be stale.
    // The list asks again for what is still on screen.
    for (const { key } of wanted) requestedAt.delete(askedKey(key, automated));
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
