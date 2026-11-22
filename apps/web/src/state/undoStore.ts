import { create } from "zustand";

import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";

type UndoRun = () => Promise<boolean>;

export interface UndoState {
  lastMutationId: string | null;
  lastUndo: UndoRun | null;
  lastUndoAt: number;
  pendingSendCancel: (() => void) | null;
  pendingSendAt: number;
  recordUndo: (run: UndoRun, mutationId?: string) => void;
  recordNoUndo: () => void;
  retireUndo: (run: UndoRun) => void;
  setLastMutationId: (id: string) => void;
  setLastUndo: (undo: UndoRun | null) => void;
  setPendingSendCancel: (cancel: (() => void) | null) => void;
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

export function runLatestUndo(): "send" | "mail" | null {
  const state = useUndo.getState();
  const sendIsNewer =
    state.pendingSendCancel && (!state.lastUndo || state.pendingSendAt >= state.lastUndoAt);
  if (sendIsNewer && state.pendingSendCancel) {
    state.pendingSendCancel();
    return "send";
  }
  if (state.lastUndo) {
    if (!refuseWhileDaemonDown("undo")) void state.lastUndo();
    return "mail";
  }
  return null;
}
