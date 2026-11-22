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

export function usePageKeys(ids: readonly string[]): void {
  const owner = useId();
  const key = ids.join("\u0000");
  useEffect(() => {
    const { set, clear } = usePageKeysStore.getState();
    if (key) set(owner, key.split("\u0000"));
    else clear(owner);
  }, [key, owner]);
  useEffect(() => () => usePageKeysStore.getState().clear(owner), [owner]);
}
