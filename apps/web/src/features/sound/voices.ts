/*
 * One synthesised voice per sound name in the daemon's chime setting, so
 * the web speaks the same vocabulary the TUI plays natively. No files: each
 * voice is a few sine or triangle partials with soft envelopes, low in the
 * mix and under a quarter of a second. `low_tide` is the web's own figure
 * for a cleared desk: two notes rising a fifth.
 */

import type { ChimeSoundName } from "./api";

export type Voice = Exclude<ChimeSoundName, "none"> | "low_tide";

export interface Partial {
  wave: "sine" | "triangle";
  /** Hz at the start; `to` glides there over the partial's length. */
  from: number;
  to?: number;
  /** Seconds after the voice starts. */
  at: number;
  /** Seconds, including the release. */
  length: number;
  /** Relative level, 0 to 1, before the volume setting. */
  level: number;
}

/** No voice runs past this, so a sound never outlasts the action. */
export const MAX_VOICE_SECONDS = 0.25;

export const VOICES: Record<Voice, readonly Partial[]> = {
  // Out it goes: a short upward glide.
  sent: [
    { wave: "sine", from: 523, to: 784, at: 0, length: 0.16, level: 1 },
    { wave: "sine", from: 1046, to: 1568, at: 0, length: 0.12, level: 0.18 },
  ],
  // Tucked away: a low, soft step down.
  archive: [{ wave: "triangle", from: 392, to: 294, at: 0, length: 0.14, level: 0.9 }],
  pop: [{ wave: "sine", from: 660, to: 440, at: 0, length: 0.09, level: 0.8 }],
  glass: [
    { wave: "sine", from: 1318, at: 0, length: 0.22, level: 0.55 },
    { wave: "sine", from: 1976, at: 0, length: 0.16, level: 0.2 },
  ],
  bell: [
    { wave: "sine", from: 880, at: 0, length: 0.24, level: 0.7 },
    { wave: "sine", from: 1760, at: 0, length: 0.14, level: 0.15 },
  ],
  thud: [{ wave: "sine", from: 150, to: 95, at: 0, length: 0.16, level: 1 }],
  alert: [
    { wave: "triangle", from: 587, at: 0, length: 0.1, level: 0.7 },
    { wave: "triangle", from: 440, at: 0.1, length: 0.12, level: 0.7 },
  ],
  low_tide: [
    { wave: "sine", from: 392, at: 0, length: 0.15, level: 0.8 },
    { wave: "sine", from: 587, at: 0.09, length: 0.16, level: 0.8 },
  ],
};
