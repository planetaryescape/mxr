/*
 * To do's web keys. The `e` alias matches the TUI; Space follows checkbox convention.
 * The mounted runway, catch-up or Expired list registers what each command
 * does (`useScopeController`); the reader keeps its own keys while it has
 * focus. `g x` opens To do from anywhere and `t` (in verbActions) makes a
 * to-do from a conversation.
 */

import { CalendarClock, Check, ListTodo, PenLine, X } from "lucide-react";

import { getRuntimeNavigate } from "@/lib/actions/runtime";
import type { Action, ActionScope, CommandAction } from "@/lib/actions/types";

function key(
  scope: ActionScope,
  id: string,
  command: string,
  label: string,
  shortcut: string,
  extra: Partial<Omit<CommandAction, "id" | "command" | "run">> = {},
): CommandAction {
  return { id, command, label, shortcut, group: "Triage", scopes: [scope], ...extra };
}

const move = (scope: ActionScope, prefix: string): Action[] => [
  key(scope, `${prefix}.down`, "down", "Next", "j", {
    group: "Move",
    aliases: ["ArrowDown"],
    hideInPalette: true,
  }),
  key(scope, `${prefix}.up`, "up", "Previous", "k", {
    group: "Move",
    aliases: ["ArrowUp"],
    hideInPalette: true,
  }),
];

export const todoActions: Action[] = [
  {
    id: "nav.todo",
    label: "To do",
    description: "Things email asked you to do, ordered by when to act",
    group: "Navigate",
    icon: ListTodo,
    shortcut: "g x",
    run: () => getRuntimeNavigate().navigate("/todo"),
  },
  ...move("todo", "todo"),
  key("todo", "todo.primary", "primary", "Do it: what the row's button says", "Enter", {
    shortLabel: "Do it",
    tuiNote: "Opens a link only when the sender passed the check; otherwise the email",
  }),
  key("todo", "todo.done", "done", "Tick off", "Space", {
    shortLabel: "Tick off",
    icon: Check,
    aliases: ["e"],
  }),
  key("todo", "todo.schedule", "schedule", "Schedule…", "Z", {
    shortLabel: "Schedule",
    icon: CalendarClock,
  }),
  key("todo", "todo.edit", "edit", "Edit what mxr read…", ",", {
    shortLabel: "Edit",
    icon: PenLine,
  }),
  key("todo", "todo.dismiss", "dismiss", "Not a to-do", "X", {
    shortLabel: "Not a to-do",
    icon: X,
  }),
  key("todo", "todo.source", "source", "Show the email it came from", "o", {
    shortLabel: "Email",
  }),
  key("todo", "todo.expired", "expired", "Expired list", "E", { shortLabel: "Expired" }),
  key("todo", "todo.catchup", "catchup", "Catch-up", "C", { shortLabel: "Catch-up" }),
  key("todo", "todo.close-hint", "closeHint", "Dismiss the hint", "Escape", {
    hideInPalette: true,
  }),
  ...move("catchup", "catchup"),
  key("catchup", "todo.catchup-keep", "keep", "Keep: it still needs me", "Enter", {
    shortLabel: "Keep",
  }),
  key("catchup", "todo.catchup-let-go", "letGo", "Let go of this one", "e", {
    shortLabel: "Let go",
  }),
  key("catchup", "todo.catchup-let-go-all", "letGoAll", "Let go of all…", "A", {
    shortLabel: "Let go of all",
    tuiNote: "Previews the daemon's dry run first; undo afterwards",
  }),
  key("catchup", "todo.catchup-back", "back", "Back to To do", "Escape", { hideInPalette: true }),
  ...move("expired", "expired"),
  key("expired", "todo.restore", "restore", "Restore to To do", "Enter", { shortLabel: "Restore" }),
  key("expired", "todo.expired-back", "back", "Back to To do", "Escape", { hideInPalette: true }),
];
