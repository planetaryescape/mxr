import { MXR_VERSION, withoutExpiredMoves, type DefinedAction } from "@/lib/actions/definition";
import keymap from "@/lib/actions/keymap.generated.json";

export function announced(id: string, key: string, place: string): boolean {
  return withoutExpiredMoves(keymap.action as DefinedAction[], MXR_VERSION).some(
    (action) =>
      action.id === id &&
      (action.moved ?? []).some((move) => move.key === key && move.contexts.includes(place)),
  );
}
