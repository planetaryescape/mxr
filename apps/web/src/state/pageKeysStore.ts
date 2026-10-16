/*
 * The page's three to five main verbs, for the status bar's key strip
 * (docs/blueprint/27 "Keys"). A page calls `usePageKeys([...action ids])`;
 * the status bar resolves each id through the action registry, so the key and
 * the verb come from the one table `?` and the palette read. Cleared when the
 * page unmounts, and a page can't clear another page's keys.
 */

import { useEffect, useId } from "react";
import { create } from "zustand";

interface PageKeysState {
  owner: string | null;
  ids: readonly string[];
  set: (owner: string, ids: readonly string[]) => void;
  clear: (owner: string) => void;
}

const NONE: readonly string[] = [];

export const usePageKeysStore = create<PageKeysState>((set) => ({
  owner: null,
  ids: NONE,
  set: (owner, ids) => set({ owner, ids }),
  clear: (owner) => set((state) => (state.owner === owner ? { owner: null, ids: NONE } : state)),
}));

/**
 * Name the action ids whose keys this page shows in the status bar, in the
 * order they should drop from the end when space is short. Pass the verbs
 * that apply to what is lit now; an empty list falls back to the registry's
 * own live keys.
 */
export function usePageKeys(ids: readonly string[]): void {
  const owner = useId();
  // A string key, so a fresh array each render never re-runs the effect.
  const key = ids.join("\u0000");
  useEffect(() => {
    const { set, clear } = usePageKeysStore.getState();
    if (key) set(owner, key.split("\u0000"));
    else clear(owner);
  }, [key, owner]);
  useEffect(() => () => usePageKeysStore.getState().clear(owner), [owner]);
}
