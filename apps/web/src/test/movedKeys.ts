import { MXR_VERSION, withoutExpiredMoves, type DefinedAction } from "@/lib/actions/definition";
import keymap from "@/lib/actions/keymap.generated.json";

/**
 * True while this build still announces `id`'s old `key` in `place`. From
 * the move's `until` release on it is ignored (blueprint 35, D171), so a
 * test of one real move expects its notice only while this holds, and
 * nothing at all after.
 */
export function announced(id: string, key: string, place: string): boolean {
  return withoutExpiredMoves(keymap.action as DefinedAction[], MXR_VERSION).some(
    (action) =>
      action.id === id &&
      (action.moved ?? []).some((move) => move.key === key && move.contexts.includes(place)),
  );
}
