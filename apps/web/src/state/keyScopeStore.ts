import { create } from "zustand";

import type { ActionScope } from "@/lib/actions/types";

export const DEAD_END_NOTICE_MS = 2000;

export interface KeyScopeState {
  stack: ActionScope[];
  pendingPrefix: string | null;
  deadEnd: string | null;
  moved: { label: string; key: string | null } | null;
  pushScope: (scope: ActionScope) => void;
  popScope: (scope: ActionScope) => void;
  activeScope: () => ActionScope;
  setPendingPrefix: (prefix: string | null) => void;
  showDeadEnd: (sequence: string) => void;
  showMoved: (label: string, key: string | null) => void;
}

let deadEndTimer: ReturnType<typeof setTimeout> | undefined;
let movedTimer: ReturnType<typeof setTimeout> | undefined;

export const useKeyScope = create<KeyScopeState>((set, get) => ({
  stack: [],
  pendingPrefix: null,
  deadEnd: null,
  moved: null,
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
  showMoved: (label, key) => {
    clearTimeout(movedTimer);
    set({ moved: { label, key } });
    movedTimer = setTimeout(() => set({ moved: null }), DEAD_END_NOTICE_MS * 2);
  },
}));
