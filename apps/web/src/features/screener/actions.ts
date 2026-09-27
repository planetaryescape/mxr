/*
 * Screener decisions on the keyboard: the TUI's screener modal keys.
 */

import type { Action } from "@/lib/actions/types";

function decision(id: string, command: string, label: string, shortcut: string): Action {
  return { id, command, label, shortcut, group: "Triage", scopes: ["screener"] };
}

export const screenerActions: Action[] = [
  decision("screener.allow", "allow", "Allow sender", "a"),
  decision("screener.deny", "deny", "Deny sender", "d"),
  decision("screener.feed", "feed", "Send to feed", "f"),
  decision("screener.paper-trail", "paperTrail", "Send to paper trail", "p"),
  {
    id: "screener.down",
    command: "down",
    label: "Next sender",
    shortcut: "j",
    aliases: ["ArrowDown"],
    group: "Move",
    scopes: ["screener"],
    hideInPalette: true,
  },
  {
    id: "screener.up",
    command: "up",
    label: "Previous sender",
    shortcut: "k",
    aliases: ["ArrowUp"],
    group: "Move",
    scopes: ["screener"],
    hideInPalette: true,
  },
];
