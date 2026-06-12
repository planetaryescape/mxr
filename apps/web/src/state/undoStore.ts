/*
 * Last undoable mail mutation, recorded on every successful mutation that
 * returns a mutation_id. The global `z` shortcut undoes it; the daemon
 * enforces the real undo window (~60s), so a stale id just fails with a
 * clear error.
 */

import { create } from "zustand";

export interface UndoState {
  lastMutationId: string | null;
  setLastMutationId: (id: string) => void;
  clear: () => void;
}

export const useUndo = create<UndoState>((set) => ({
  lastMutationId: null,
  setLastMutationId: (id) => set({ lastMutationId: id }),
  clear: () => set({ lastMutationId: null }),
}));
