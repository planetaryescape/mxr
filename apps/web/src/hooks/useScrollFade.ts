import { useEffect, useState } from "react";

/**
 * A strip that scrolls sideways should say so: `fade` is true while content
 * runs off its right edge, so the caller can fade that edge instead of
 * cutting a word. `ref` is a callback ref (held in state) so a strip that
 * mounts after the first render is still measured.
 */
export function useScrollFade<T extends HTMLElement>() {
  const [scroller, ref] = useState<T | null>(null);
  const [fade, setFade] = useState(false);
  useEffect(() => {
    if (!scroller) return;
    const measure = () =>
      setFade(scroller.scrollWidth - scroller.clientWidth - scroller.scrollLeft > 4);
    measure();
    scroller.addEventListener("scroll", measure, { passive: true });
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(scroller);
    return () => {
      scroller.removeEventListener("scroll", measure);
      observer?.disconnect();
    };
  }, [scroller]);
  return { ref, fade };
}
