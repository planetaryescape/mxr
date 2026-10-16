/*
 * Change landing (docs/blueprint/27 "Feedback", D134): when a change the
 * person didn't make on this page lands (the assistant applied a proof),
 * the rows it changed wear a 2px signal underline that fades over 600ms.
 *
 * `land(ids)` marks message or thread ids; a row asks `useLanded(ids)` and
 * re-renders only when its own answer changes, so a long list never
 * re-renders for it. The mark clears itself after the duration.
 */

import { useCallback } from "react";
import { create } from "zustand";

/** Matches `--motion-duration-land` in styles/tokens.css. */
export const LANDING_MS = 600;

interface LandingState {
  landed: ReadonlySet<string>;
  land: (ids: readonly string[]) => void;
}

export const useLanding = create<LandingState>((set, get) => ({
  landed: new Set(),
  land: (ids) => {
    if (ids.length === 0) return;
    set({ landed: new Set([...get().landed, ...ids]) });
    setTimeout(() => {
      const next = new Set(get().landed);
      for (const id of ids) next.delete(id);
      set({ landed: next });
    }, LANDING_MS);
  },
}));

/** True while any of these ids is landing. Pass stable, primitive ids. */
export function useLanded(...ids: ReadonlyArray<string | null | undefined>): boolean {
  const key = ids.join("\u0000");
  const select = useCallback(
    (state: LandingState) =>
      state.landed.size > 0 && ids.some((id) => id != null && state.landed.has(id)),
    // `key` stands for the ids by value.
    // oxlint-disable-next-line react-hooks/exhaustive-deps
    [key],
  );
  return useLanding(select);
}
