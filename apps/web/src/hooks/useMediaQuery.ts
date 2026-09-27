import { useSyncExternalStore } from "react";

/** Live `matchMedia` result; false where matchMedia is unavailable. */
export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      if (typeof window === "undefined" || !window.matchMedia) return () => {};
      const media = window.matchMedia(query);
      media.addEventListener("change", onChange);
      return () => media.removeEventListener("change", onChange);
    },
    () =>
      typeof window !== "undefined" && window.matchMedia ? window.matchMedia(query).matches : false,
    () => false,
  );
}

/** Below this width the sidebar shows icons only (matches app.css). */
export const NARROW_SHELL_QUERY = "(max-width: 1279px)";
/** Below this width list and reader never share the screen. */
export const SINGLE_PANE_QUERY = "(max-width: 1023px)";
