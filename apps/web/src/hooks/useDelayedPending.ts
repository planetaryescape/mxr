/*
 * The loading-state rule: local data usually lands in well under 300 ms, and
 * a skeleton that flashes for a frame is worse than nothing. So a load shows
 * nothing at first ("quiet"), a skeleton only once it has taken 300 ms, and
 * a skeleton that did appear stays at least 400 ms so it never flickers.
 * While a skeleton holds, the caller keeps showing it even if data is ready.
 */

import { useEffect, useState } from "react";

export const PENDING_DELAY_MS = 300;
export const PENDING_MINIMUM_MS = 400;

/** "quiet": loading, show an empty frame; "skeleton": show it; "ready": show the content. */
export type PendingPhase = "ready" | "quiet" | "skeleton";

export function useDelayedPending(
  pending: boolean,
  delay = PENDING_DELAY_MS,
  minimum = PENDING_MINIMUM_MS,
): PendingPhase {
  const [shownAt, setShownAt] = useState<number | null>(null);
  useEffect(() => {
    if (pending && shownAt === null) {
      const timer = setTimeout(() => setShownAt(Date.now()), delay);
      return () => clearTimeout(timer);
    }
    if (!pending && shownAt !== null) {
      const left = Math.max(0, shownAt + minimum - Date.now());
      const timer = setTimeout(() => setShownAt(null), left);
      return () => clearTimeout(timer);
    }
    return undefined;
  }, [delay, minimum, pending, shownAt]);
  if (shownAt !== null) return "skeleton";
  return pending ? "quiet" : "ready";
}
