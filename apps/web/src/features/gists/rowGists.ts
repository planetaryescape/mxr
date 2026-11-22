import { useCallback, useSyncExternalStore } from "react";

import type { DaemonEvent } from "@/api/events";
import {
  fetchThreadGists,
  type AiProvenance,
  type GistModel,
  type ThreadGist,
  type ThreadGistBatch,
} from "@/features/thread/context/api";
import { provenanceLabel } from "@/features/thread/context/contextFormat";
import { isDaemonDown } from "@/lib/daemonAvailability";
import { daemonEvents } from "@/lib/ws";

export interface RowGist {
  about: string;
  ask: string | null;
  source: string;
  provenance: AiProvenance | null;
  newest: string | null;
}

export const MAX_BATCH = 100;
export const STATE_CAP = 500;
const REQUEST_TTL_MS = 60_000;
const NO_MODEL_RECHECK_MS = 5 * 60_000;

const gists = new Map<string, RowGist>();
const requestedAt = new Map<string, number>();
const askedKey = (threadId: string, automated: boolean) =>
  `${automated ? "automated" : "people"}:${threadId}`;
function forgetAsked(threadId: string): void {
  requestedAt.delete(askedKey(threadId, false));
  requestedAt.delete(askedKey(threadId, true));
}
const listeners = new Map<string, Set<() => void>>();
let epoch = 0;
const epochListeners = new Set<() => void>();
let noModelUntil = 0;
let model: GistModel | undefined;
const modelListeners = new Set<() => void>();
let unsubscribeEvents: (() => void) | undefined;

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

function notify(threadId: string): void {
  for (const listener of listeners.get(threadId) ?? []) listener();
}

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

export function putGist(data: ThreadGist): void {
  const gist = toRowGist(data);
  if (!gist) return;
  const shown = gists.get(data.thread_id);
  const same =
    shown?.about === gist.about &&
    shown.ask === gist.ask &&
    shown.source === gist.source &&
    shown.newest === gist.newest;
  for (const evicted of remember(gists, data.thread_id, same ? shown : gist)) notify(evicted);
  if (!same) notify(data.thread_id);
}

function bumpEpoch(): void {
  epoch += 1;
  for (const listener of epochListeners) listener();
}

export function forgetGists(threadIds: Iterable<string>): void {
  for (const threadId of threadIds) {
    forgetAsked(threadId);
    if (gists.delete(threadId)) notify(threadId);
  }
  bumpEpoch();
}

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
      const { envelopes = [], total = 0 } = event as {
        envelopes?: Array<{ thread_id?: unknown }>;
        total?: number;
      };
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
      invalidateRowGists();
      break;
  }
}

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

export async function requestRowGists(
  threadIds: readonly string[],
  now = Date.now(),
  { automated = false }: { automated?: boolean } = {},
): Promise<void> {
  if (now < noModelUntil) return;
  listen();
  if (isDaemonDown()) return;
  const wanted: string[] = [];
  for (const threadId of new Set(threadIds)) {
    const asked =
      now - (requestedAt.get(askedKey(threadId, automated)) ?? -Infinity) < REQUEST_TTL_MS;
    if (gists.has(threadId) || asked) continue;
    wanted.push(threadId);
    if (wanted.length === MAX_BATCH) break;
  }
  if (wanted.length === 0) return;
  for (const threadId of wanted) remember(requestedAt, askedKey(threadId, automated), now);
  const sentIn = epoch;

  let batch: ThreadGistBatch;
  try {
    batch = await fetchThreadGists(wanted, true, automated);
  } catch {
    for (const threadId of wanted) requestedAt.delete(askedKey(threadId, automated));
    return;
  }
  if (sentIn !== epoch) {
    for (const threadId of wanted) requestedAt.delete(askedKey(threadId, automated));
    return;
  }
  setModel(batch.model);
  if (batch.model === "disabled") {
    noModelUntil = now + NO_MODEL_RECHECK_MS;
    return;
  }
  for (const gist of batch.gists) putGist(gist);
}

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
