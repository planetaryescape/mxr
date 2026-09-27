import { useEffect } from "react";

import { useComposeUi } from "@/features/compose/composeUiStore";
import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import { getRegistry, snapshotActionContext } from "@/lib/actions";
import { installKeyDispatcher } from "@/lib/keys/dispatcher";
import { useKeyScope } from "@/state/keyScopeStore";
import { useModals } from "@/state/modalStore";

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
        isSuspended: () => {
          // A palette or dialog owns the keyboard from the moment it opens,
          // not only once its input has focus: fast typing after ⌘K would
          // otherwise land on the page for a frame.
          const modals = useModals.getState();
          if (modals.commandPaletteOpen || modals.searchPaletteOpen || modals.helpOpen) return true;
          if (useMailDialogs.getState().dialog) return true;
          const compose = useComposeUi.getState();
          if (compose.intent && compose.surface !== "inline") return true;
          return window.location.pathname.startsWith("/compose");
        },
      }),
    [],
  );
}
