/*
 * Last undoable mail mutation, recorded on every successful mutation that
 * returns a mutation_id. The global `z` shortcut undoes it; the daemon
 * enforces the real undo window (~60s), so a stale id just fails with a
 * clear error.
 */

import { create } from "zustand";

export interface UndoState {
  lastMutationId: string | null;
  /** Cancels the most recent undo-send window, if one is still open.
   * The global `z` action prefers this over mutation undo. */
  pendingSendCancel: (() => void) | null;
  setLastMutationId: (id: string) => void;
  setPendingSendCancel: (cancel: (() => void) | null) => void;
  clear: () => void;
}

export const useUndo = create<UndoState>((set) => ({
  lastMutationId: null,
  pendingSendCancel: null,
  setLastMutationId: (id) => set({ lastMutationId: id }),
  setPendingSendCancel: (pendingSendCancel) => set({ pendingSendCancel }),
  clear: () => set({ lastMutationId: null }),
}));
