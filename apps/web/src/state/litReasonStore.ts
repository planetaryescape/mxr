/*
 * The lit row's reason, for the status bar. A page with a lit row calls
 * `useLitReason(reason)`; the status bar reads the store. The page never
 * renders the reason in the row itself, so lighting a row can't change its
 * height (docs/blueprint/27 "Rows").
 */

import { useEffect, useId } from "react";
import { create } from "zustand";

interface LitReasonState {
  /** Which page set it, so a page unmounting never clears another page's reason. */
  owner: string | null;
  reason: string | null;
  /** A second clause after the reason, such as a new sender's key choices. */
  note: string | null;
  set: (owner: string, reason: string, note: string | null) => void;
  clear: (owner: string) => void;
}

export const useLitReasonStore = create<LitReasonState>((set) => ({
  owner: null,
  reason: null,
  note: null,
  set: (owner, reason, note) => set({ owner, reason, note }),
  clear: (owner) =>
    set((state) => (state.owner === owner ? { owner: null, reason: null, note: null } : state)),
}));

/**
 * Tell the status bar why the lit row is here. Pass null when nothing is lit
 * (or the lit row has no reason): the status bar goes back to its freshness
 * line. The reason also clears when the page unmounts.
 */
export function useLitReason(reason: string | null | undefined, note?: string | null): void {
  const owner = useId();
  useEffect(() => {
    const { set, clear } = useLitReasonStore.getState();
    if (reason) set(owner, reason, note ?? null);
    else clear(owner);
  }, [note, owner, reason]);
  useEffect(() => () => useLitReasonStore.getState().clear(owner), [owner]);
}
