import { useEffect } from "react";

import { useComposeUi } from "@/features/compose/composeUiStore";
import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import { getRegistry, snapshotActionContext } from "@/lib/actions";
import { installKeyDispatcher } from "@/lib/keys/dispatcher";
import { useKeyScope } from "@/state/keyScopeStore";
import { useKeymapStore } from "@/state/keymapStore";
import { useModals } from "@/state/modalStore";

/**
 * True while a palette, dialog or compose owns the keyboard. The desktop
 * menu's commands wait on the same check, as a key press would.
 */
export function shortcutsSuspended(): boolean {
  // A palette or dialog owns the keyboard from the moment it opens,
  // not only once its input has focus: fast typing after ⌘K would
  // otherwise land on the page for a frame.
  const modals = useModals.getState();
  if (modals.commandPaletteOpen || modals.searchPaletteOpen || modals.helpOpen) return true;
  if (useMailDialogs.getState().dialog) return true;
  const compose = useComposeUi.getState();
  if (compose.intent && compose.surface !== "inline") return true;
  return window.location.pathname.startsWith("/compose");
}

/**
 * Install the app's single keydown dispatcher. Compose in an overlay or
 * fullscreen owns the keyboard: a stray "e" outside the editor must not
 * archive the thread behind it.
 */
export function useKeyDispatcher(): void {
  useEffect(
    () =>
      installKeyDispatcher(window, {
        registry: getRegistry(),
        context: snapshotActionContext,
        onPendingChange: (prefix) => useKeyScope.getState().setPendingPrefix(prefix),
        onDeadEnd: (sequence) => useKeyScope.getState().showDeadEnd(sequence),
        onMoved: (action, key) =>
          useKeyScope
            .getState()
            .showMoved((action.shortLabel ?? action.label).replace(/…$/, ""), key),
        isSuspended: shortcutsSuspended,
        singleKeysOff: () => !useKeymapStore.getState().singleKeys,
      }),
    [],
  );
}
