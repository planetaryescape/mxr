import { useEffect } from "react";
import type {
  DiagnosticsWorkspaceSection,
  FocusContext,
  WorkbenchScreen,
} from "../../shared/types";
import type {
  DesktopAction,
  DesktopBindingContext,
  PendingBinding,
} from "../lib/tui-manifest";
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
  submitCompose?: (action: "send" | "save") => void;
  closeAllDialogs: () => void;
  setFocusContext: (context: FocusContext) => void;
  selectedMessageIds: Set<string>;
  visualMode: boolean;
  diagnosticsScreenShortcuts: Record<
    string,
    DiagnosticsWorkspaceSection
  > | null;
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
    submitCompose,
    closeAllDialogs,
    setFocusContext,
    selectedMessageIds,
    visualMode,
    diagnosticsScreenShortcuts,
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
            setSelectedCommandIndex(
              Math.min(selectedCommandIndex + 1, filteredCommandCount - 1),
            );
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
        if (composeOpen) {
          if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            submitCompose?.("send");
            return;
          }
          if (event.key === "s" && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            submitCompose?.("save");
            return;
          }
        }
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

      if (
        screen === "diagnostics" &&
        diagnosticsScreenShortcuts &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey
      ) {
        const diagnosticsSection = diagnosticsScreenShortcuts[event.key];
        if (diagnosticsSection) {
          event.preventDefault();
          setPendingBinding(null);
          dispatchAction(`open_diagnostics_section:${diagnosticsSection}`);
          return;
        }
      }

      if (
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        event.key === "/"
      ) {
        event.preventDefault();
        setPendingBinding(null);
        dispatchAction("search_all_mail");
        return;
      }

      if (
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        event.key === "o" &&
        (focusContext === "mailList" || focusContext === "search")
      ) {
        event.preventDefault();
        setPendingBinding(null);
        dispatchAction("open_focus_reader");
        return;
      }

      if (
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        event.key === "l" &&
        focusContext === "reader"
      ) {
        event.preventDefault();
        setPendingBinding(null);
        dispatchAction("apply_label");
        return;
      }

      const token = normalizeKeyToken(event);
      if (!token) {
        return;
      }

      const now = Date.now();
      const next = resolveBindingAction(
        bindingContext,
        token,
        pendingBinding,
        now,
      );
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
    diagnosticsScreenShortcuts,
    setFocusContext,
    setSelectedCommandIndex,
    setPendingBinding,
    visualMode,
  ]);
}
