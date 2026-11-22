import { useEffect, useState } from "react";

export const PENDING_DELAY_MS = 150;
export const PENDING_MINIMUM_MS = 400;

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

export function useHeldPending(pending: boolean): boolean {
  return useDelayedPending(pending) !== "ready";
}
