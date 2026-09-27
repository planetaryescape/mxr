/*
 * Modal / right-rail / command-palette open state.
 * Independent of TanStack Router so we can keep these ephemeral.
 */

import { create } from "zustand";

export interface ModalState {
  commandPaletteOpen: boolean;
  /** "lens" opens the palette as a label / saved-search jumper (`g l`). */
  commandPaletteMode: "all" | "lens";
  searchPaletteOpen: boolean;
  composeLauncherOpen: boolean;
  rightRail: { kind: string; payload?: unknown } | null;
  helpOpen: boolean;
  setCommandPaletteOpen: (open: boolean) => void;
  openCommandPaletteAt: (mode: "all" | "lens") => void;
  setSearchPaletteOpen: (open: boolean) => void;
  setComposeLauncherOpen: (open: boolean) => void;
  openRightRail: (kind: string, payload?: unknown) => void;
  closeRightRail: () => void;
  setHelpOpen: (open: boolean) => void;
}

export const useModals = create<ModalState>((set) => ({
  commandPaletteOpen: false,
  commandPaletteMode: "all",
  searchPaletteOpen: false,
  composeLauncherOpen: false,
  rightRail: null,
  helpOpen: false,
  setCommandPaletteOpen: (commandPaletteOpen) =>
    set(
      commandPaletteOpen
        ? { commandPaletteOpen }
        : { commandPaletteOpen, commandPaletteMode: "all" },
    ),
  openCommandPaletteAt: (commandPaletteMode) =>
    set({ commandPaletteOpen: true, commandPaletteMode }),
  setSearchPaletteOpen: (searchPaletteOpen) => set({ searchPaletteOpen }),
  setComposeLauncherOpen: (composeLauncherOpen) => set({ composeLauncherOpen }),
  openRightRail: (kind, payload) => set({ rightRail: { kind, payload } }),
  closeRightRail: () => set({ rightRail: null }),
  setHelpOpen: (helpOpen) => set({ helpOpen }),
}));
