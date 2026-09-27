/*
 * The most recent reversible action. `u` / `z` undo whichever is newest: a
 * send still in its undo window, or the last mail change. A mail change
 * that cannot be undone (star, labels, move) retires the previous undo, so
 * `u` never reaches back past it to an older action.
 */

import { create } from "zustand";

type UndoRun = () => Promise<boolean>;

export interface UndoState {
  /** Daemon mutation id behind `lastUndo`, when it has one. */
  lastMutationId: string | null;
  lastUndo: UndoRun | null;
  lastUndoAt: number;
  /** Cancels the most recent undo-send window, if one is still open. */
  pendingSendCancel: (() => void) | null;
  pendingSendAt: number;
  recordUndo: (run: UndoRun, mutationId?: string) => void;
  /** A newer mail change happened that has no undo. */
  recordNoUndo: () => void;
  /** Clear `run` only if it is still the newest; older toasts can't wipe newer undo. */
  retireUndo: (run: UndoRun) => void;
  setLastMutationId: (id: string) => void;
  setLastUndo: (undo: UndoRun | null) => void;
  setPendingSendCancel: (cancel: (() => void) | null) => void;
  /** Clear the pending-send cancel only if it is still `cancel`, so one
   * compose session finishing never drops another session's undo. */
  clearPendingSendCancel: (cancel: () => void) => void;
  clear: () => void;
}

export const useUndo = create<UndoState>((set, get) => ({
  lastMutationId: null,
  lastUndo: null,
  lastUndoAt: 0,
  pendingSendCancel: null,
  pendingSendAt: 0,
  recordUndo: (run, mutationId) =>
    set({ lastUndo: run, lastUndoAt: Date.now(), lastMutationId: mutationId ?? null }),
  recordNoUndo: () => set({ lastUndo: null, lastMutationId: null }),
  retireUndo: (run) => {
    if (get().lastUndo === run) set({ lastUndo: null, lastMutationId: null });
  },
  setLastMutationId: (id) => set({ lastMutationId: id }),
  setLastUndo: (lastUndo) =>
    set({ lastUndo, lastUndoAt: lastUndo ? Date.now() : get().lastUndoAt }),
  setPendingSendCancel: (pendingSendCancel) =>
    set({ pendingSendCancel, pendingSendAt: pendingSendCancel ? Date.now() : get().pendingSendAt }),
  clearPendingSendCancel: (cancel) => {
    if (get().pendingSendCancel === cancel) set({ pendingSendCancel: null });
  },
  clear: () => set({ lastMutationId: null, lastUndo: null }),
}));

/** Run the newest undo: an open send window or the last mail change. */
export function runLatestUndo(): "send" | "mail" | null {
  const state = useUndo.getState();
  const sendIsNewer =
    state.pendingSendCancel && (!state.lastUndo || state.pendingSendAt >= state.lastUndoAt);
  if (sendIsNewer && state.pendingSendCancel) {
    state.pendingSendCancel();
    return "send";
  }
  if (state.lastUndo) {
    void state.lastUndo();
    return "mail";
  }
  return null;
}
