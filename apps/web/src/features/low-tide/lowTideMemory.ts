/*
 * "Once per clearing": a place remembers, per browser, whether it last had
 * work in it. The moment shows on the first view after that work is gone,
 * whether it was cleared here or in another client, and never on later
 * visits to a place that was already clear. Storage may be unavailable
 * (private windows, blocked site data): then the moment simply never shows.
 */

import { useEffect, useState } from "react";

export type TidePlace =
  | "desk"
  | "reply_queue"
  | "reading"
  | "paper_trail"
  // The modes that end in a finishing moment (D134): once per mode per day.
  | "messages"
  | "todo"
  | "updates";

/** What the place shows right now. */
export type TideState = "loading" | "work" | "clear";

const key = (place: TidePlace) => `mxr.lowTide.${place}`;
const dayKey = (place: TidePlace) => `mxr.lowTide.day.${place}`;

/** The viewer's local calendar day, so "once a day" turns over at their midnight. */
export function localDay(now: Date = new Date()): string {
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const date = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${date}`;
}

function read(name: string): string | null {
  try {
    return window.localStorage.getItem(name);
  } catch {
    return null;
  }
}

function write(name: string, value: string): void {
  try {
    window.localStorage.setItem(name, value);
  } catch {
    // Unavailable storage only costs the moment, never the page.
  }
}

/** Remember that the place has work in it. */
export function noteWork(place: TidePlace): void {
  if (read(key(place)) !== "work") write(key(place), "work");
}

/**
 * The place is clear now. True exactly once per clearing: when it last had
 * work. Marks the clearing seen either way.
 */
export function claimLowTide(place: TidePlace, daily?: string): boolean {
  const earned = read(key(place)) === "work";
  if (read(key(place)) !== "clear") write(key(place), "clear");
  if (!earned || daily === undefined) return earned;
  // A clearing earned today after the moment already played today shows only the words.
  if (read(dayKey(place)) === daily) return false;
  write(dayKey(place), daily);
  return true;
}

/**
 * Whether to show the moment for `place`. It stays up while the place stays
 * clear on this visit, and goes when work comes back. `daily` limits it to
 * once per place per local day.
 */
export function useLowTide(
  place: TidePlace,
  loading: boolean,
  hasWork: boolean,
  { daily = false }: { daily?: boolean } = {},
): boolean {
  const state: TideState = loading ? "loading" : hasWork ? "work" : "clear";
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (state === "work") {
      noteWork(place);
      setShown(false);
    } else if (state === "clear" && claimLowTide(place, daily ? localDay() : undefined)) {
      setShown(true);
    }
  }, [place, state, daily]);
  return shown;
}
