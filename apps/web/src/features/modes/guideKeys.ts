import type { ModeGuide, ModeKey } from "./api";

import { chordsOf, getRegistry } from "@/lib/actions/registry";

/** Add web-only aliases to the shared guide at the point of display. */
export function keysForModeGuide(guide: Pick<ModeGuide, "mode" | "keys">): ModeKey[] {
  if (guide.mode !== "todo") return guide.keys;
  const done = getRegistry().get("todo.done");
  if (!done) return guide.keys;

  const doneKeys = chordsOf(done);
  if (!guide.keys.some((key) => key.key === "e")) return guide.keys;

  return guide.keys.flatMap((key) =>
    key.key === "e" ? doneKeys.map((chord) => ({ key: chord, verb: key.verb })) : [key],
  );
}
