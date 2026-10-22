/*
 * Screener decisions on the keyboard: the TUI's screener modal keys.
 */

import type { ActionBehavior } from "@/lib/actions/types";

export const screenerActions: ActionBehavior[] = [
  { id: "screener.allow", command: "allow" },
  { id: "screener.deny", command: "deny" },
  { id: "screener.feed", command: "feed" },
  { id: "screener.paper-trail", command: "paperTrail" },
  { id: "screener.down", command: "down" },
  { id: "screener.up", command: "up" },
];
