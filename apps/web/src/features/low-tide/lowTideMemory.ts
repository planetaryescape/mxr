/*
 * "Once per clearing": a place remembers, per browser, whether it last had
 * work in it. The moment shows on the first view after that work is gone,
 * whether it was cleared here or in another client, and never on later
 * visits to a place that was already clear. Storage may be unavailable
 * (private windows, blocked site data): then the moment simply never shows.
 */

import { useEffect, useState } from "react";

export type TidePlace = "desk" | "reading" | "paper_trail";

/** What the place shows right now. */
export type TideState = "loading" | "work" | "clear";

const key = (place: TidePlace) => `mxr.lowTide.${place}`;

function read(place: TidePlace): string | null {
  try {
    return window.localStorage.getItem(key(place));
  } catch {
    return null;
  }
}

function write(place: TidePlace, value: "work" | "clear"): void {
  try {
    window.localStorage.setItem(key(place), value);
  } catch {
    // Unavailable storage only costs the moment, never the page.
  }
}

/** Remember that the place has work in it. */
export function noteWork(place: TidePlace): void {
  if (read(place) !== "work") write(place, "work");
}

/**
 * The place is clear now. True exactly once per clearing: when it last had
 * work. Marks the clearing seen either way.
 */
export function claimLowTide(place: TidePlace): boolean {
  const earned = read(place) === "work";
  if (read(place) !== "clear") write(place, "clear");
  return earned;
}

/**
 * Whether to show the moment for `place`. It stays up while the place stays
 * clear on this visit, and goes when work comes back.
 */
export function useLowTide(place: TidePlace, loading: boolean, hasWork: boolean): boolean {
  const state: TideState = loading ? "loading" : hasWork ? "work" : "clear";
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (state === "work") {
      noteWork(place);
      setShown(false);
    } else if (state === "clear" && claimLowTide(place)) {
      setShown(true);
    }
  }, [place, state]);
  return shown;
}
