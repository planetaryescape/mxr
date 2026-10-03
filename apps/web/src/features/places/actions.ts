/*
 * Keys in Updates (built on Paper trail) and Reading. The mounted place view registers what
 * each command does (`useScopeController("place", …)`); the reader keeps
 * its own keys while it has focus.
 */

import { Archive, Check, Pin, Shuffle } from "lucide-react";

import type { Action, CommandAction } from "@/lib/actions/types";

function placeAction(
  id: string,
  command: string,
  label: string,
  shortcut: string,
  extra: Partial<Omit<CommandAction, "id" | "command" | "run">> = {},
): CommandAction {
  return { id, command, label, shortcut, group: "Triage", scopes: ["place"], ...extra };
}

export const placeActions: Action[] = [
  placeAction("place.down", "down", "Next", "j", {
    shortLabel: "Next",
    group: "Move",
    aliases: ["ArrowDown"],
    hideInPalette: true,
  }),
  placeAction("place.up", "up", "Previous", "k", {
    shortLabel: "Previous",
    group: "Move",
    aliases: ["ArrowUp"],
    hideInPalette: true,
  }),
  placeAction("place.open", "open", "Open, or expand a bundle", "Enter", {
    shortLabel: "Open",
    group: "Move",
    aliases: ["o"],
    hideInPalette: true,
  }),
  placeAction("place.done", "done", "Done here: let go of this in Updates or Reading", "e", {
    shortLabel: "Done here",
    icon: Check,
    description: "On a sender's row in Updates, every message of theirs shown here",
  }),
  placeAction("place.pin", "pin", "Pin or unpin (a sweep leaves pins)", "p", {
    shortLabel: "Pin",
    icon: Pin,
  }),
  // S and A, as in the TUI: s means star in every mail list.
  placeAction("place.sweep-bundle", "sweepBundle", "Sweep this sender's bundle…", "S", {
    shortLabel: "Sweep sender",
    icon: Archive,
    tuiNote: "Previews the daemon's dry run first; undo afterwards",
  }),
  placeAction("place.sweep-all", "sweepAll", "Sweep the whole place…", "A", {
    shortLabel: "Sweep all",
    icon: Archive,
    tuiNote: "Everything unpinned here; previews first and opens on Cancel (Tab, then Enter)",
  }),
  placeAction("place.move-sender", "moveSender", "Move sender to…", "K", {
    icon: Shuffle,
    tuiNote:
      "In the reader K is the previous message; use the palette or the line under the thread",
    description:
      "People, Reading, Paper trail, Screened out or automatic, for their future mail too",
  }),
  placeAction("place.unsubscribe", "unsubscribe", "Unsubscribe…", "D"),
  {
    id: "reader.move-sender",
    command: "moveSender",
    label: "Move sender to…",
    description:
      "People, Reading, Paper trail, Screened out or automatic, for their future mail too",
    group: "Triage",
    icon: Shuffle,
    scopes: ["reader"],
    paletteOnly: true,
  },
];
