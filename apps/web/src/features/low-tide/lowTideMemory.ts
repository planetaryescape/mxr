import { useEffect, useState } from "react";

export type TidePlace =
  | "desk"
  | "reply_queue"
  | "reading"
  | "paper_trail"
  | "now"
  | "messages"
  | "todo"
  | "updates";

export type TideState = "loading" | "work" | "clear";

const key = (place: TidePlace) => `mxr.lowTide.${place}`;
const dayKey = (place: TidePlace) => `mxr.lowTide.day.${place}`;

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
  } catch {}
}

export function noteWork(place: TidePlace): void {
  if (read(key(place)) !== "work") write(key(place), "work");
}

export function claimLowTide(place: TidePlace, daily?: string): boolean {
  const earned = read(key(place)) === "work";
  if (read(key(place)) !== "clear") write(key(place), "clear");
  if (!earned || daily === undefined) return earned;
  if (read(dayKey(place)) === daily) return false;
  write(dayKey(place), daily);
  return true;
}

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
