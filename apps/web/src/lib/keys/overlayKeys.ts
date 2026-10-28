/*
 * Overlay keys (blueprint 35, "Overlays share one grammar"). An open dialog
 * owns every key, so the dispatcher stands aside and the dialog reads its
 * keys here, by action id, from the key definition. A dialog never spells a
 * key itself, so the TUI, help and the keybindings page can't disagree
 * with it.
 */

import { definedKeys, definedOverlayKeys, movedOverlayKey } from "@/lib/actions/definition";
import { useKeyScope } from "@/state/keyScopeStore";

import { isMacPlatform, tokenFromEvent } from "./chord";

type KeyPress = Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

/**
 * True when `event` is one of overlay action `id`'s keys. The definition
 * spells the TUI's Ctrl; off macOS the browser's Ctrl is `Mod`.
 */
export function pressedOverlayKey(event: KeyPress, id: string, mac = isMacPlatform()): boolean {
  const token = tokenFromEvent(event, mac);
  if (!token) return false;
  return definedOverlayKeys(id, mac).some(
    (chord) => chord === token || (!mac && chord.replace(/^Ctrl\+/, "Mod+") === token),
  );
}

/**
 * When `event` is a key that moved away from `overlay`, say where its action
 * went (as the dispatcher does for page keys) and return true.
 */
export function announceMovedOverlayKey(
  event: KeyPress,
  overlay: string,
  mac = isMacPlatform(),
): boolean {
  const token = tokenFromEvent(event, mac);
  const action = token ? movedOverlayKey(overlay, token) : undefined;
  if (!action) return false;
  useKeyScope.getState().showMoved(action.label, action.keys?.[0] ?? null);
  return true;
}

/** The letter an option prints on itself (blueprint 35's Pick rule): its first one-character key. */
export function optionLetter(id: string): string {
  const keys = definedKeys(id);
  return keys.find((key) => key.length === 1) ?? keys[0] ?? "";
}
