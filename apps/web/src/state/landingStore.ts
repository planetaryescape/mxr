import { useCallback } from "react";
import { create } from "zustand";

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

export function useLanded(...ids: ReadonlyArray<string | null | undefined>): boolean {
  const key = ids.join("\u0000");
  const select = useCallback(
    (state: LandingState) =>
      state.landed.size > 0 && ids.some((id) => id != null && state.landed.has(id)),
    // oxlint-disable-next-line react-hooks/exhaustive-deps
    [key],
  );
  return useLanding(select);
}
