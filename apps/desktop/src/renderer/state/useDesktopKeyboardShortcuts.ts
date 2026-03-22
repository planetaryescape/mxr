import { useEffect } from "react";
import type { FocusContext, WorkbenchScreen } from "../../shared/types";
import type { DesktopAction, DesktopBindingContext, PendingBinding } from "../lib/tui-manifest";
import { resolveBindingAction } from "../lib/tui-manifest";
import { isTypingTarget, normalizeKeyToken } from "./desktop-actions";

export function useDesktopKeyboardShortcuts(props: {
  bindingContext: DesktopBindingContext;
  pendingBinding: PendingBinding | null;
  setPendingBinding: (binding: PendingBinding | null) => void;
  commandPaletteOpen: boolean;
  screen: WorkbenchScreen;
  modalOpen: boolean;
  composeOpen: boolean;
  closeComposeShell: () => void;
  closeAllDialogs: () => void;
  setFocusContext: (context: FocusContext) => void;
  selectedMessageIds: Set<string>;
  visualMode: boolean;
  dispatchAction: (action: DesktopAction | string) => void;
}) {
  const {
    bindingContext,
    pendingBinding,
    setPendingBinding,
    commandPaletteOpen,
    screen,
    modalOpen,
    composeOpen,
    closeComposeShell,
    closeAllDialogs,
    setFocusContext,
    selectedMessageIds,
    visualMode,
    dispatchAction,
  } = props;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) {
        return;
      }

      const activeElement = document.activeElement;
      const typing = isTypingTarget(activeElement);
      if (typing && event.key !== "Escape" && !event.ctrlKey) {
        return;
      }

      if (typing && event.key === "Escape") {
        event.preventDefault();
        (activeElement as HTMLElement | null)?.blur?.();
        setPendingBinding(null);
        if (commandPaletteOpen) {
          dispatchAction("quit_view");
          return;
        }
        setFocusContext(screen === "search" ? "search" : "mailList");
        return;
      }

      if (modalOpen) {
        if (event.key === "Escape") {
          event.preventDefault();
          if (composeOpen) {
            closeComposeShell();
            return;
          }
          closeAllDialogs();
          setFocusContext(screen === "search" ? "search" : "mailList");
        }
        return;
      }

      if (event.key === "Escape") {
        event.preventDefault();
        setPendingBinding(null);
        if (selectedMessageIds.size > 0 || visualMode) {
          dispatchAction("clear_selection");
          return;
        }
        dispatchAction("quit_view");
        return;
      }

      const token = normalizeKeyToken(event);
      if (!token) {
        return;
      }

      const now = Date.now();
      const next = resolveBindingAction(bindingContext, token, pendingBinding, now);
      if (!next) {
        return;
      }

      event.preventDefault();
      setPendingBinding(next.pending ?? null);
      if (next.action) {
        dispatchAction(next.action);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    bindingContext,
    closeAllDialogs,
    closeComposeShell,
    commandPaletteOpen,
    composeOpen,
    dispatchAction,
    modalOpen,
    pendingBinding,
    screen,
    selectedMessageIds,
    setFocusContext,
    setPendingBinding,
    visualMode,
  ]);
}
