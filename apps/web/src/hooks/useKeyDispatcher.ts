import { useEffect } from "react";

import { useComposeUi } from "@/features/compose/composeUiStore";
import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import { getRegistry, snapshotActionContext } from "@/lib/actions";
import { installKeyDispatcher } from "@/lib/keys/dispatcher";
import { useKeyScope } from "@/state/keyScopeStore";
import { useKeymapStore } from "@/state/keymapStore";
import { useModals } from "@/state/modalStore";

export function shortcutsSuspended(): boolean {
  const modals = useModals.getState();
  if (modals.commandPaletteOpen || modals.searchPaletteOpen || modals.helpOpen) return true;
  if (useMailDialogs.getState().dialog) return true;
  const compose = useComposeUi.getState();
  if (compose.intent && compose.surface !== "inline") return true;
  return window.location.pathname.startsWith("/compose");
}

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
