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
  filteredCommandCount: number;
  selectedCommandIndex: number;
  setSelectedCommandIndex: (index: number) => void;
  runSelectedCommand: () => void;
  screen: WorkbenchScreen;
  focusContext: FocusContext;
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
    filteredCommandCount,
    selectedCommandIndex,
    setSelectedCommandIndex,
    runSelectedCommand,
    screen,
    focusContext,
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
    const openCommandPalette = () => {
      setPendingBinding(null);
      dispatchAction("command_palette");
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) {
        return;
      }

      if (commandPaletteOpen) {
        if (event.key === "Enter") {
          event.preventDefault();
          runSelectedCommand();
          return;
        }

        if (
          !event.metaKey &&
          !event.ctrlKey &&
          !event.altKey &&
          (event.key === "j" || event.key === "ArrowDown")
        ) {
          event.preventDefault();
          if (filteredCommandCount > 0) {
            setSelectedCommandIndex(Math.min(selectedCommandIndex + 1, filteredCommandCount - 1));
          }
          return;
        }

        if (
          !event.metaKey &&
          !event.ctrlKey &&
          !event.altKey &&
          (event.key === "k" || event.key === "ArrowUp")
        ) {
          event.preventDefault();
          if (filteredCommandCount > 0) {
            setSelectedCommandIndex(Math.max(selectedCommandIndex - 1, 0));
          }
          return;
        }
      }

      const activeElement = document.activeElement;
      const typing = isTypingTarget(activeElement);
      if (
        typing &&
        screen === "search" &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        event.key === "Enter"
      ) {
        event.preventDefault();
        (activeElement as HTMLElement | null)?.blur?.();
        setPendingBinding(null);
        setFocusContext("search");
        return;
      }
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

      if (event.metaKey && !event.ctrlKey && event.key.toLowerCase() === "p") {
        event.preventDefault();
        openCommandPalette();
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

      if (
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        (event.key === "h" || event.key === "ArrowLeft")
      ) {
        if (focusContext === "reader") {
          event.preventDefault();
          setPendingBinding(null);
          setFocusContext(screen === "search" ? "search" : "mailList");
          return;
        }
        if (focusContext === "mailList" || focusContext === "search") {
          event.preventDefault();
          setPendingBinding(null);
          setFocusContext("sidebar");
          return;
        }
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
    window.addEventListener("mxr:command-palette", openCommandPalette);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("mxr:command-palette", openCommandPalette);
    };
  }, [
    bindingContext,
    closeAllDialogs,
    closeComposeShell,
    commandPaletteOpen,
    composeOpen,
    dispatchAction,
    filteredCommandCount,
    focusContext,
    modalOpen,
    pendingBinding,
    runSelectedCommand,
    screen,
    selectedCommandIndex,
    selectedMessageIds,
    setFocusContext,
    setSelectedCommandIndex,
    setPendingBinding,
    visualMode,
  ]);
}
