/*
 * Focus & reply keys. `g F` opens it from anywhere. Inside, the keys act
 * on the conversation on screen; the reply's own ⌘Enter sends and moves on.
 */

import { Target } from "lucide-react";

import { getRuntimeNavigate } from "@/lib/actions/runtime";
import type { ActionBehavior } from "@/lib/actions/types";

/** Where focus mode returns to on Esc: the page it was opened from. */
export function focusPath(from = window.location.pathname + window.location.search): string {
  if (from.startsWith("/focus")) return "/focus";
  // From the desk, focus works through its You owe lane.
  const lane = from.startsWith("/desk") ? "lane=owed&" : "";
  return `/focus?${lane}from=${encodeURIComponent(from)}`;
}

export const focusActions: ActionBehavior[] = [
  { id: "nav.focus", icon: Target, run: () => getRuntimeNavigate().navigate(focusPath()) },
  { id: "focus.send", command: "send" },
  { id: "focus.skip", command: "skip" },
  { id: "focus.done", command: "done" },
  { id: "focus.snooze", command: "snooze" },
  { id: "focus.remind", command: "remind" },
  { id: "focus.draft", command: "draft" },
  { id: "focus.reply", command: "reply" },
  { id: "focus.leave", command: "leave" },
];
