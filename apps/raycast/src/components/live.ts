import { useEffect, useRef } from "react";

/**
 * How often an open list asks the daemon again. Raycast keeps a command's
 * view alive for a while after it closes, so without this a list reopened
 * soon after, or left open, would miss mail the daemon has since synced.
 * The daemon answers from views it keeps ready, in milliseconds.
 */
const EVERY_MS = 20_000;

export function useLiveRefresh(revalidate: () => unknown, active = true): void {
  const latest = useRef(revalidate);
  latest.current = revalidate;
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => void latest.current(), EVERY_MS);
    return () => clearInterval(timer);
  }, [active]);
}
