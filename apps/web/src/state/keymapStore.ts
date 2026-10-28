/*
 * The person's key settings, as the daemon serves them (`GetKeymap`,
 * blueprint chapter 35): whether single-character shortcuts are on, which
 * actions have keys of their own, and what in `keys.toml` mxr ignores.
 * `version` changes each time the registry is rebuilt with new keys, so
 * views that list keys recompute.
 */

import { create } from "zustand";

export interface KeymapState {
  /** False when `keys.toml` turns single-character shortcuts off (WCAG 2.1.4). */
  singleKeys: boolean;
  /** Ids of the actions whose keys the person changed. */
  remapped: string[];
  /** What in `keys.toml` mxr is ignoring. */
  problems: string[];
  version: number;
  setServed: (served: { singleKeys: boolean; remapped: string[]; problems: string[] }) => void;
}

export const useKeymapStore = create<KeymapState>((set) => ({
  singleKeys: true,
  remapped: [],
  problems: [],
  version: 0,
  setServed: (served) => set((state) => ({ ...served, version: state.version + 1 })),
}));

/** True for a key token the single-key switch turns off: one character on its own. */
export function isCharacterKey(chord: string): boolean {
  const first = chord.split(" ")[0] ?? "";
  return [...first].length === 1;
}

/** True when the single-key switch turns this key off right now. */
export function isSwitchedOff(chord: string): boolean {
  return !useKeymapStore.getState().singleKeys && isCharacterKey(chord);
}
