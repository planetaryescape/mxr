import { Mail } from "lucide-react";

import { MODE_GLYPHS } from "@/components/ModeMark";
import type { ThreadModes } from "@/features/modes/membership";

export type ModesByThread = ReadonlyMap<string, ThreadModes> | undefined;

export function threadGlyph(threadId: string | null, modes: ModesByThread) {
  const held = threadId ? modes?.get(threadId) : undefined;
  const mode = held?.modes[0]?.mode ?? (held?.in_inbox ? "inbox" : null);
  return mode ? MODE_GLYPHS[mode] : Mail;
}
