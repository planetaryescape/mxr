/*
 * Active shortcut-scope stack plus the pending multi-key prefix indicator.
 * Views push their scope on mount (useShortcutScope); the keymap reads the
 * top of the stack at dispatch time. Page components surface their pending
 * sequence prefix ("g", "*") here so the status bar can show it.
 */

import { create } from "zustand";

import type { ActionScope } from "@/lib/actions/types";

export interface KeyScopeState {
  stack: ActionScope[];
  pendingPrefix: string | null;
  pushScope: (scope: ActionScope) => void;
  popScope: (scope: ActionScope) => void;
  activeScope: () => ActionScope;
  setPendingPrefix: (prefix: string | null) => void;
}

export const useKeyScope = create<KeyScopeState>((set, get) => ({
  stack: [],
  pendingPrefix: null,
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
}));
