/*
 * The extension's shortcuts, from mxr's one key definition
 * (crates/keymap/keymap.toml, blueprint chapter 35). Each is Cmd plus the
 * letter the same action has in the TUI and the web app, so a key moved
 * there moves here. Regenerate with `UPDATE_KEYMAP=1 cargo test -p mxr-keymap`.
 */

import type { Keyboard } from "@raycast/api";

import keymap from "../keymap.generated.json";

type Defined = { modifiers: Keyboard.KeyModifier[]; key: Keyboard.KeyEquivalent };

const keys = keymap as Record<string, Defined>;

/** The shortcut for an action id in the definition. */
export function keyFor(id: string): Keyboard.Shortcut {
  const shortcut = keys[id];
  if (!shortcut) throw new Error(`No Raycast key for "${id}" in crates/keymap/keymap.toml`);
  return shortcut;
}
