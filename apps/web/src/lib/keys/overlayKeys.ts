import { definedKeys, definedOverlayKeys, movedOverlayKey } from "@/lib/actions/definition";
import { useKeyScope } from "@/state/keyScopeStore";

import { isMacPlatform, tokenFromEvent } from "./chord";

type KeyPress = Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

export function pressedOverlayKey(event: KeyPress, id: string, mac = isMacPlatform()): boolean {
  const token = tokenFromEvent(event, mac);
  if (!token) return false;
  return definedOverlayKeys(id, mac).some(
    (chord) => chord === token || (!mac && chord.replace(/^Ctrl\+/, "Mod+") === token),
  );
}

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

export function optionLetter(id: string): string {
  const keys = definedKeys(id);
  return keys.find((key) => key.length === 1) ?? keys[0] ?? "";
}
