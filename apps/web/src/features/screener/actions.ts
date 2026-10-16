/*
 * Screener decisions on the keyboard: the TUI's screener modal keys.
 */

import type { Action } from "@/lib/actions/types";

/**
 * The lit row's choice is a number (1 Allow, 2 Deny, 3 Feed, 4 Paper trail),
 * like a new sender's; the TUI modal's letters answer the same way.
 */
function decision(
  id: string,
  command: string,
  label: string,
  number: string,
  letter: string,
): Action {
  return {
    id,
    command,
    label,
    shortcut: number,
    aliases: [letter],
    group: "Triage",
    scopes: ["screener"],
  };
}

export const screenerActions: Action[] = [
  decision("screener.allow", "allow", "Allow sender", "1", "a"),
  decision("screener.deny", "deny", "Deny sender", "2", "d"),
  decision("screener.feed", "feed", "Send to feed", "3", "f"),
  decision("screener.paper-trail", "paperTrail", "Send to paper trail", "4", "p"),
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
