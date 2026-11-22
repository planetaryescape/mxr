import { useEffect } from "react";
import { toast } from "sonner";

import { getRegistry, snapshotActionContext } from "@/lib/actions";
import { invokeAction, isAvailable, type ActionRegistry } from "@/lib/actions/registry";
import type { ActionContext } from "@/lib/actions/types";

import { shortcutsSuspended } from "./useKeyDispatcher";

export const MENU_ACTION_EVENT = "mxr:menu-action";

declare global {
  interface Window {
    mxrMenuReady?: boolean;
  }
}

export function runMenuAction(
  id: string,
  registry: ActionRegistry = getRegistry(),
  context: () => ActionContext = snapshotActionContext,
  suspended: () => boolean = shortcutsSuspended,
): void {
  const action = registry.get(id);
  if (!action || suspended()) return;
  const ctx = context();
  if (!isAvailable(action, ctx)) {
    toast(`${(action.shortLabel ?? action.label).replace(/…$/, "")} isn't available here`);
    return;
  }
  invokeAction(action, ctx, "pointer");
}

export function useDesktopMenu(): void {
  useEffect(() => {
    const run = (event: Event) => {
      if (event instanceof CustomEvent && typeof event.detail === "string") {
        runMenuAction(event.detail);
      }
    };
    window.addEventListener(MENU_ACTION_EVENT, run);
    window.mxrMenuReady = true;
    return () => {
      window.removeEventListener(MENU_ACTION_EVENT, run);
      window.mxrMenuReady = false;
    };
  }, []);
}
