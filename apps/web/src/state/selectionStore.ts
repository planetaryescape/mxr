/*
 * Bulk-selection state, keyed by the current mailbox URL.
 * Cleared automatically on mailbox change.
 */

import { create } from "zustand";

export interface SelectionState {
  scope: string | null;
  ids: Set<string>;
  lastClickedId: string | null;
  setScope: (scope: string) => void;
  toggle: (id: string) => void;
  selectRange: (ids: string[]) => void;
  selectMany: (ids: string[]) => void;
  clear: () => void;
  has: (id: string) => boolean;
  size: () => number;
}

export const useSelection = create<SelectionState>((set, get) => ({
  scope: null,
  ids: new Set(),
  lastClickedId: null,
  setScope: (scope) => {
    if (get().scope !== scope) {
      set({ scope, ids: new Set(), lastClickedId: null });
    }
  },
  toggle: (id) => {
    set((s) => {
      const next = new Set(s.ids);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return { ids: next, lastClickedId: id };
    });
  },
  selectRange: (ids) => {
    set((s) => {
      const next = new Set(s.ids);
      for (const id of ids) next.add(id);
      return { ids: next };
    });
  },
  selectMany: (ids) => {
    set({ ids: new Set(ids) });
  },
  clear: () => set({ ids: new Set(), lastClickedId: null }),
  has: (id) => get().ids.has(id),
  size: () => get().ids.size,
}));
