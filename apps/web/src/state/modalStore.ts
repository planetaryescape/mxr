import { create } from "zustand";

export interface ModalState {
  commandPaletteOpen: boolean;
  commandPaletteMode: "all" | "lens";
  searchPaletteOpen: boolean;
  composeLauncherOpen: boolean;
  rightRail: { kind: string; payload?: unknown } | null;
  helpOpen: boolean;
  moreSheetOpen: boolean;
  setMoreSheetOpen: (open: boolean) => void;
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
  moreSheetOpen: false,
  setMoreSheetOpen: (moreSheetOpen) => set({ moreSheetOpen }),
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
