import { useEffect } from "react";

import type { ActionScope } from "@/lib/actions/types";
import { useKeyScope } from "@/state/keyScopeStore";

export function useShortcutScope(scope: ActionScope, active = true): void {
  const pushScope = useKeyScope((s) => s.pushScope);
  const popScope = useKeyScope((s) => s.popScope);
  useEffect(() => {
    if (!active) return;
    pushScope(scope);
    return () => popScope(scope);
  }, [scope, active, pushScope, popScope]);
}
