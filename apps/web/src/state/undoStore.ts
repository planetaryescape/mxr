/*
 * The most recent reversible action. `u` / `z` run it: a pending undo-send
 * first, then the last mail change. Mutations with a daemon mutation id undo
 * through the daemon (its window is about 60 seconds, so a stale id fails
 * with a clear error); snoozes undo by waking the messages.
 */

import { create } from "zustand";

export interface UndoState {
  lastMutationId: string | null;
  /** Undo for the most recent mail change, whatever its mechanism. */
  lastUndo: (() => Promise<boolean>) | null;
  /** Cancels the most recent undo-send window, if one is still open. */
  pendingSendCancel: (() => void) | null;
  setLastMutationId: (id: string) => void;
  setLastUndo: (undo: (() => Promise<boolean>) | null) => void;
  setPendingSendCancel: (cancel: (() => void) | null) => void;
  /** Clear the pending-send cancel only if it is still `cancel`, so one
   * compose session finishing never drops another session's undo. */
  clearPendingSendCancel: (cancel: () => void) => void;
  clear: () => void;
}

export const useUndo = create<UndoState>((set, get) => ({
  lastMutationId: null,
  lastUndo: null,
  pendingSendCancel: null,
  setLastMutationId: (id) => set({ lastMutationId: id }),
  setLastUndo: (lastUndo) => set({ lastUndo }),
  setPendingSendCancel: (pendingSendCancel) => set({ pendingSendCancel }),
  clearPendingSendCancel: (cancel) => {
    if (get().pendingSendCancel === cancel) set({ pendingSendCancel: null });
  },
  clear: () => set({ lastMutationId: null, lastUndo: null }),
}));
