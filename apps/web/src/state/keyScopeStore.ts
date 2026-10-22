/*
 * Active shortcut-scope stack plus the pending multi-key prefix indicator.
 * Views push their scope on mount (useShortcutScope); the keymap reads the
 * top of the stack at dispatch time. Page components surface their pending
 * sequence prefix ("g", "*") here so the status bar can show it.
 */

import { create } from "zustand";

import type { ActionScope } from "@/lib/actions/types";

/** How long "g U is not a shortcut" stays in the status bar. */
export const DEAD_END_NOTICE_MS = 2000;

export interface KeyScopeState {
  stack: ActionScope[];
  pendingPrefix: string | null;
  /** A prefix and key that completed nothing ("g U"), shown briefly. */
  deadEnd: string | null;
  pushScope: (scope: ActionScope) => void;
  popScope: (scope: ActionScope) => void;
  activeScope: () => ActionScope;
  setPendingPrefix: (prefix: string | null) => void;
  showDeadEnd: (sequence: string) => void;
}

let deadEndTimer: ReturnType<typeof setTimeout> | undefined;

export const useKeyScope = create<KeyScopeState>((set, get) => ({
  stack: [],
  pendingPrefix: null,
  deadEnd: null,
  pushScope: (scope) => set((s) => ({ stack: [...s.stack, scope] })),
  popScope: (scope) =>
    set((s) => {
      const index = s.stack.lastIndexOf(scope);
      if (index < 0) return s;
      const stack = [...s.stack];
      stack.splice(index, 1);
      return { stack };
    }),
  activeScope: () => get().stack.at(-1) ?? "global",
  setPendingPrefix: (prefix) => set({ pendingPrefix: prefix }),
  showDeadEnd: (sequence) => {
    clearTimeout(deadEndTimer);
    set({ deadEnd: sequence });
    deadEndTimer = setTimeout(() => set({ deadEnd: null }), DEAD_END_NOTICE_MS);
  },
}));
