import { useEffect } from "react";

import type { ActionScope } from "@/lib/actions/types";
import { useKeyScope } from "@/state/keyScopeStore";

/**
 * Declare the shortcut scope owned by the mounted view. Pushed on mount,
 * popped on unmount; the most recently mounted scope wins at dispatch time.
 * Pass `active: false` to temporarily yield (e.g. pane not focused).
 */
export function useShortcutScope(scope: ActionScope, active = true): void {
  const pushScope = useKeyScope((s) => s.pushScope);
  const popScope = useKeyScope((s) => s.popScope);
  useEffect(() => {
    if (!active) return;
    pushScope(scope);
    return () => popScope(scope);
  }, [scope, active, pushScope, popScope]);
}
