import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Respects prefers-reduced-motion.
 */
export function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(() =>
    typeof window !== "undefined"
      ? window.matchMedia("(prefers-reduced-motion: reduce)").matches
      : false,
  );

  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const handler = (e: MediaQueryListEvent) => setReduced(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  return reduced;
}

/**
 * Manages exit animation for list items.
 * Returns removingIds and a trigger function.
 * When triggered, items animate out, then are removed from data after delay.
 */
export function useExitAnimation(delay = 150) {
  const [removingIds, setRemovingIds] = useState<Set<string>>(new Set());
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const triggerExit = useCallback(
    (ids: Set<string>, onComplete: () => void) => {
      setRemovingIds(new Set(ids));
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        setRemovingIds(new Set());
        onComplete();
      }, delay);
    },
    [delay],
  );

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  return { removingIds, triggerExit } as const;
}
