/*
 * Focus & reply keys. `g F` opens it from anywhere (the TUI leaves g+F
 * free; lower-case f is forward). Inside, the keys act on the conversation
 * on screen; the reply's own ⌘Enter sends and moves on.
 */

import { Target } from "lucide-react";

import { getRuntimeNavigate } from "@/lib/actions/runtime";
import type { Action } from "@/lib/actions/types";

/** Where focus mode returns to on Esc: the page it was opened from. */
export function focusPath(from = window.location.pathname + window.location.search): string {
  if (from.startsWith("/focus")) return "/focus";
  // From the desk, focus works through its You owe lane.
  const lane = from.startsWith("/desk") ? "lane=owed&" : "";
  return `/focus?${lane}from=${encodeURIComponent(from)}`;
}

function focusKey(
  id: string,
  command: string,
  label: string,
  shortcut: string,
  extra: Partial<Omit<Action, "id" | "command" | "run">> = {},
): Action {
  return { id, command, label, shortcut, group: "Compose", scopes: ["focus"], ...extra } as Action;
}

export const focusActions: Action[] = [
  {
    id: "nav.focus",
    label: "Focus & reply",
    description: "Reply to everyone waiting on you, one conversation at a time",
    group: "Compose",
    icon: Target,
    shortcut: "g F",
    run: () => getRuntimeNavigate().navigate(focusPath()),
    tuiNote: "F in the TUI's reply queue",
  },
  focusKey("focus.send", "send", "Send and next", "Mod+Enter", { shortLabel: "Send, next" }),
  focusKey("focus.skip", "skip", "Skip for now", "s", {
    shortLabel: "Skip",
    tuiNote: "Goes to the end of the queue",
  }),
  focusKey("focus.snooze", "snooze", "Snooze…", "Z"),
  focusKey("focus.remind", "remind", "Send, remind me if nobody replies…", "w", {
    shortLabel: "Send, remind",
  }),
  focusKey("focus.draft", "draft", "Draft in your voice", "d", { shortLabel: "Draft" }),
  focusKey("focus.reply", "reply", "Back to the reply", "r", { hideInPalette: true }),
  focusKey("focus.leave", "leave", "Leave focus mode", "Escape", { hideInPalette: true }),
];
