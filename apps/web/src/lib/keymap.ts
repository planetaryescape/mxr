/*
 * Global keymap built from the action registry. Page-level chords (j/k, x,
 * etc.) are handled by per-page components — this only binds chords that the
 * registry exposes as non-paletteOnly actions.
 *
 * Inline-only chords (alt-bindings like Shift+Semicolon for the command
 * palette) live below the registry-derived map. The compose route disables
 * the keymap; the editor handles its own keys.
 */

import type { KeyBindingMap } from "tinykeys";

import { getRegistry, setRuntimeNavigate } from "@/lib/actions";
import type { ActionContext } from "@/lib/actions";
import { useKeyScope } from "@/state/keyScopeStore";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useModals } from "@/state/modalStore";
import { useSelection } from "@/state/selectionStore";

interface Navigator {
  navigate: (to: string) => void;
}

const THREAD_PATH_RE = /^\/m\/[^/]+\/[^/]+/;
const MESSAGE_PATH_RE = /^\/m\/[^/]+\/[^/]+\/[^/]+/;

function buildContextSnapshot(): ActionContext {
  const path = typeof window !== "undefined" ? window.location.pathname : "/";
  return {
    path,
    activePane: useMailboxPane.getState().activePane,
    selectionCount: useSelection.getState().ids.size,
    accountCount: 0,
    hasFocusedThread: THREAD_PATH_RE.test(path),
    hasFocusedMessage: MESSAGE_PATH_RE.test(path),
    isFirstAccountOnly: false,
  };
}

function suppressedInTextField(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  if (!t) return false;
  return t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable;
}

export function buildGlobalKeymap(nav: Navigator): KeyBindingMap {
  setRuntimeNavigate(nav);
  const reg = getRegistry();
  const map: KeyBindingMap = {};
  for (const [chord, byScope] of Object.entries(reg.getShortcutMap())) {
    map[chord] = (e) => {
      // A page component that already handled (and preventDefault-ed) this
      // key wins over the global binding.
      if (e.defaultPrevented) return;
      if (suppressedInTextField(e)) return;
      // Resolve at dispatch time: active scope first, then global fallback.
      const scope = useKeyScope.getState().activeScope();
      const actionId = byScope[scope] ?? byScope.global;
      const action = actionId ? reg.get(actionId) : undefined;
      if (!action) return;
      e.preventDefault();
      void action.run(buildContextSnapshot());
    };
  }
  // Alt-binding for command palette — colon (:) opens it, mirroring the TUI.
  map["Shift+Semicolon"] = (e) => {
    if (suppressedInTextField(e)) return;
    e.preventDefault();
    useModals.getState().setCommandPaletteOpen(true);
  };
  return map;
}
