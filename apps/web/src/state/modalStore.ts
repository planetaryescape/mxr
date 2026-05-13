/*
 * Modal / right-rail / command-palette open state.
 * Independent of TanStack Router so we can keep these ephemeral.
 */

import { create } from "zustand";

export interface ModalState {
  commandPaletteOpen: boolean;
  searchPaletteOpen: boolean;
  composeLauncherOpen: boolean;
  rightRail: { kind: string; payload?: unknown } | null;
  helpOpen: boolean;
  setCommandPaletteOpen: (open: boolean) => void;
  setSearchPaletteOpen: (open: boolean) => void;
  setComposeLauncherOpen: (open: boolean) => void;
  openRightRail: (kind: string, payload?: unknown) => void;
  closeRightRail: () => void;
  setHelpOpen: (open: boolean) => void;
}

export const useModals = create<ModalState>((set) => ({
  commandPaletteOpen: false,
  searchPaletteOpen: false,
  composeLauncherOpen: false,
  rightRail: null,
  helpOpen: false,
  setCommandPaletteOpen: (commandPaletteOpen) => set({ commandPaletteOpen }),
  setSearchPaletteOpen: (searchPaletteOpen) => set({ searchPaletteOpen }),
  setComposeLauncherOpen: (composeLauncherOpen) => set({ composeLauncherOpen }),
  openRightRail: (kind, payload) => set({ rightRail: { kind, payload } }),
  closeRightRail: () => set({ rightRail: null }),
  setHelpOpen: (helpOpen) => set({ helpOpen }),
}));
