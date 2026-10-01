/*
 * One shared clock for relative times ("Just now", "12m", "Yesterday").
 * Each date on screen subscribes on its own and re-renders only when its
 * own words change, so a list never re-renders on a tick. The clock runs
 * only while something on screen subscribes.
 */

import { useSyncExternalStore } from "react";

const TICK_MS = 30_000;
const listeners = new Set<() => void>();
let now = Date.now();
let timer: ReturnType<typeof setInterval> | undefined;

function tick(): void {
  now = Date.now();
  for (const listener of listeners) listener();
}

function onVisible(): void {
  if (document.visibilityState === "visible") tick();
}

function subscribe(listener: () => void): () => void {
  if (listeners.size === 0) {
    now = Date.now();
    timer = setInterval(tick, TICK_MS);
    document.addEventListener("visibilitychange", onVisible);
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (listeners.size > 0) return;
    clearInterval(timer);
    timer = undefined;
    document.removeEventListener("visibilitychange", onVisible);
  };
}

/** The clock's current time: the last tick while it runs, else the real time. */
function current(): number {
  return listeners.size > 0 ? now : Date.now();
}

/**
 * Words that depend on the time, kept current: re-renders the caller only
 * when `format`'s result changes (at most every 30 s).
 */
export function useClockLabel(format: (now: Date) => string): string {
  return useSyncExternalStore(subscribe, () => format(new Date(current())));
}
