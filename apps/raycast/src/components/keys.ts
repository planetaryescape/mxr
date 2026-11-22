import type { Keyboard } from "@raycast/api";

import keymap from "../keymap.generated.json";

type Defined = { modifiers: Keyboard.KeyModifier[]; key: Keyboard.KeyEquivalent };

const keys = keymap as Record<string, Defined>;

export function keyFor(id: string): Keyboard.Shortcut {
  const shortcut = keys[id];
  if (!shortcut) throw new Error(`No Raycast key for "${id}" in crates/keymap/keymap.toml`);
  return shortcut;
}
