import { create } from "zustand";

export interface KeymapState {
  singleKeys: boolean;
  remapped: string[];
  problems: string[];
  version: number;
  catalogLoaded: boolean;
  setServed: (served: { singleKeys: boolean; remapped: string[]; problems: string[] }) => void;
  markCatalogLoaded: () => void;
}

export const useKeymapStore = create<KeymapState>((set) => ({
  singleKeys: true,
  remapped: [],
  problems: [],
  version: 0,
  catalogLoaded: false,
  setServed: (served) => set((state) => ({ ...served, version: state.version + 1 })),
  markCatalogLoaded: () => set((state) => ({ catalogLoaded: true, version: state.version + 1 })),
}));

export function isCharacterKey(chord: string): boolean {
  const first = chord.split(" ")[0] ?? "";
  return [...first].length === 1;
}

export function isSwitchedOff(chord: string): boolean {
  return !useKeymapStore.getState().singleKeys && isCharacterKey(chord);
}
