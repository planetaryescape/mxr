/*
 * Page-level keybinding map. Vim-style sequences (g i, g s, …) plus Gmail
 * shortcuts plus our own. Compose route disables this; the editor handles
 * its own keys.
 */

import type { KeyBindingMap } from "tinykeys";

import { useModals } from "@/state/modalStore";

interface Navigator {
  navigate: (to: string) => void;
}

function openSearchPalette(e: KeyboardEvent): void {
  const t = e.target as HTMLElement | null;
  if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
  e.preventDefault();
  useModals.getState().setSearchPaletteOpen(true);
}

function navigateUnlessTyping(e: KeyboardEvent, nav: Navigator, to: string): void {
  const t = e.target as HTMLElement | null;
  if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
  e.preventDefault();
  nav.navigate(to);
}

export function buildGlobalKeymap(nav: Navigator): KeyBindingMap {
  return {
    "$mod+KeyK": (e) => {
      e.preventDefault();
      useModals.getState().setCommandPaletteOpen(true);
    },
    "Shift+Semicolon": (e) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      e.preventDefault();
      useModals.getState().setCommandPaletteOpen(true);
    },
    "/": openSearchPalette,
    Slash: openSearchPalette,
    "1": (e) => navigateUnlessTyping(e, nav, "/m/inbox"),
    "2": (e) => navigateUnlessTyping(e, nav, "/search"),
    "3": (e) => navigateUnlessTyping(e, nav, "/analytics"),
    "4": (e) => navigateUnlessTyping(e, nav, "/rules"),
    "5": (e) => navigateUnlessTyping(e, nav, "/screener"),
    "6": (e) => navigateUnlessTyping(e, nav, "/subscriptions"),
    "7": (e) => navigateUnlessTyping(e, nav, "/reply-queue"),
    "8": (e) => navigateUnlessTyping(e, nav, "/accounts"),
    "9": (e) => navigateUnlessTyping(e, nav, "/diagnostics"),
    "0": (e) => navigateUnlessTyping(e, nav, "/settings/theme"),
    Digit1: (e) => navigateUnlessTyping(e, nav, "/m/inbox"),
    Digit2: (e) => navigateUnlessTyping(e, nav, "/search"),
    Digit3: (e) => navigateUnlessTyping(e, nav, "/analytics"),
    Digit4: (e) => navigateUnlessTyping(e, nav, "/rules"),
    Digit5: (e) => navigateUnlessTyping(e, nav, "/screener"),
    Digit6: (e) => navigateUnlessTyping(e, nav, "/subscriptions"),
    Digit7: (e) => navigateUnlessTyping(e, nav, "/reply-queue"),
    Digit8: (e) => navigateUnlessTyping(e, nav, "/accounts"),
    Digit9: (e) => navigateUnlessTyping(e, nav, "/diagnostics"),
    Digit0: (e) => navigateUnlessTyping(e, nav, "/settings/theme"),
    "Shift+Slash": (e) => {
      // `?` opens help cheat sheet
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      e.preventDefault();
      const modals = useModals.getState();
      modals.setHelpOpen(!modals.helpOpen);
    },
    "g i": () => nav.navigate("/m/inbox"),
    "g s": () => nav.navigate("/m/starred"),
    "g d": () => nav.navigate("/m/drafts"),
    "g t": () => nav.navigate("/m/trash"),
    "g a": () => nav.navigate("/m/archive"),
    "g r": () => nav.navigate("/rules"),
    "g n": () => nav.navigate("/m/snoozed"),
    "g l": () => nav.navigate("/reply-queue"),
    "g u": () => nav.navigate("/subscriptions"),
    KeyC: (e) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      e.preventDefault();
      useModals.getState().setComposeLauncherOpen(true);
    },
  };
}
