import { Mail } from "lucide-react";

import { MODE_GLYPHS } from "@/components/ModeMark";
import type { ThreadModes } from "@/features/modes/membership";

export type ModesByThread = ReadonlyMap<string, ThreadModes> | undefined;

/**
 * The glyph of where an email lives: its first mode in rail order, else
 * Inbox when only the provider's inbox holds it. Mail while the lookup is
 * out or the thread is unknown (docs/blueprint/27 "Iconography").
 */
export function threadGlyph(threadId: string | null, modes: ModesByThread) {
  const held = threadId ? modes?.get(threadId) : undefined;
  const mode = held?.modes[0]?.mode ?? (held?.in_inbox ? "inbox" : null);
  return mode ? MODE_GLYPHS[mode] : Mail;
}
