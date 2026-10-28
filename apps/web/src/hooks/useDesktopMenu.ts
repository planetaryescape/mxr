import { useEffect } from "react";
import { toast } from "sonner";

import { getRegistry, snapshotActionContext } from "@/lib/actions";
import { invokeAction, isAvailable, type ActionRegistry } from "@/lib/actions/registry";
import type { ActionContext } from "@/lib/actions/types";

import { shortcutsSuspended } from "./useKeyDispatcher";

/**
 * The desktop app's menu (apps/desktop/src-tauri/src/menu.rs) dispatches
 * this with an action id once `mxrMenuReady` is set. A page script can only
 * run an action a key press could run.
 */
export const MENU_ACTION_EVENT = "mxr:menu-action";

declare global {
  interface Window {
    mxrMenuReady?: boolean;
  }
}

/**
 * Run a menu command the way its key would: through the registry, with the
 * same previews and confirms, and not while a dialog or compose owns the
 * keyboard. A command with nothing to act on here says so, since a menu
 * click that does nothing looks broken.
 */
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

/** Mounted in AppShell, so menu commands work on every page. */
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
