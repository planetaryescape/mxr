/*
 * Chords the browser keeps for itself: tabs, windows, reload, find, print,
 * and the OS's quit and hide keys. A page's keydown handler is not told
 * about these reliably, because the browser acts first or the page can't
 * cancel them, so a binding on one of them silently does nothing for some
 * users. `browserReserved.test.ts` fails if any declared binding uses one.
 *
 * Written in the app's chord grammar. `Mod` is Cmd on macOS and Ctrl
 * elsewhere; `Meta` is Cmd on macOS and the Windows key elsewhere.
 * `platforms` says where the browser keeps it. Add an entry when a browser
 * shortcut is confirmed in its docs, and say which docs in the reason.
 */

export type Platform = "mac" | "other";

export interface ReservedChord {
  chord: string;
  platforms: readonly Platform[];
  reason: string;
}

const EVERYWHERE: readonly Platform[] = ["mac", "other"];

export const BROWSER_RESERVED: readonly ReservedChord[] = [
  { chord: "Mod+w", platforms: EVERYWHERE, reason: "close tab" },
  { chord: "Mod+Shift+w", platforms: EVERYWHERE, reason: "close window" },
  { chord: "Mod+t", platforms: EVERYWHERE, reason: "new tab" },
  { chord: "Mod+Shift+t", platforms: EVERYWHERE, reason: "reopen closed tab" },
  { chord: "Mod+n", platforms: EVERYWHERE, reason: "new window" },
  { chord: "Mod+Shift+n", platforms: EVERYWHERE, reason: "new private window" },
  { chord: "Mod+q", platforms: EVERYWHERE, reason: "quit the browser" },
  { chord: "Mod+l", platforms: EVERYWHERE, reason: "focus the address bar" },
  { chord: "Mod+Tab", platforms: EVERYWHERE, reason: "switch tabs" },
  { chord: "Mod+1", platforms: EVERYWHERE, reason: "jump to tab 1" },
  { chord: "Mod+2", platforms: EVERYWHERE, reason: "jump to tab 2" },
  { chord: "Mod+3", platforms: EVERYWHERE, reason: "jump to tab 3" },
  { chord: "Mod+4", platforms: EVERYWHERE, reason: "jump to tab 4" },
  { chord: "Mod+5", platforms: EVERYWHERE, reason: "jump to tab 5" },
  { chord: "Mod+6", platforms: EVERYWHERE, reason: "jump to tab 6" },
  { chord: "Mod+7", platforms: EVERYWHERE, reason: "jump to tab 7" },
  { chord: "Mod+8", platforms: EVERYWHERE, reason: "jump to tab 8" },
  { chord: "Mod+9", platforms: EVERYWHERE, reason: "jump to the last tab" },
  { chord: "Mod+r", platforms: EVERYWHERE, reason: "reload" },
  { chord: "Mod+Shift+r", platforms: EVERYWHERE, reason: "reload, bypassing the cache" },
  { chord: "Mod+f", platforms: EVERYWHERE, reason: "find in page" },
  { chord: "Mod+p", platforms: EVERYWHERE, reason: "print" },
  { chord: "Mod+s", platforms: EVERYWHERE, reason: "save page" },
  { chord: "Mod+o", platforms: EVERYWHERE, reason: "open file" },
  { chord: "F5", platforms: EVERYWHERE, reason: "reload" },
  { chord: "F11", platforms: EVERYWHERE, reason: "full screen" },
  { chord: "F12", platforms: EVERYWHERE, reason: "developer tools" },
  { chord: "Meta+h", platforms: ["mac"], reason: "hide the app" },
  { chord: "Meta+m", platforms: ["mac"], reason: "minimise the window" },
  { chord: "Meta+,", platforms: ["mac"], reason: "preferences" },
  { chord: "Alt+F4", platforms: ["other"], reason: "close the window" },
];
