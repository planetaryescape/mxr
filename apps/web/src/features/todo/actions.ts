/*
 * To do's web keys. The mounted runway, catch-up or Expired list registers
 * what each command does (`useScopeController`); the reader keeps its own
 * keys while it has focus. `g x` opens To do from anywhere and `t` (in
 * verbActions) makes a to-do from a conversation.
 */

import { CalendarClock, Check, PenLine, X } from "lucide-react";

import { MODE_GLYPHS } from "@/components/ModeMark";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import type { ActionBehavior } from "@/lib/actions/types";

const move = (prefix: string): ActionBehavior[] => [
  { id: `${prefix}.down`, command: "down" },
  { id: `${prefix}.up`, command: "up" },
];

export const todoActions: ActionBehavior[] = [
  { id: "nav.todo", icon: MODE_GLYPHS.todo, run: () => getRuntimeNavigate().navigate("/todo") },
  ...move("todo"),
  { id: "todo.primary", command: "primary" },
  { id: "todo.done", command: "done", icon: Check },
  { id: "todo.schedule", command: "schedule", icon: CalendarClock },
  { id: "todo.edit", command: "edit", icon: PenLine },
  { id: "todo.dismiss", command: "dismiss", icon: X },
  { id: "todo.open-email", command: "openEmail" },
  { id: "todo.details", command: "details" },
  { id: "todo.expired", command: "expired" },
  { id: "todo.catchup", command: "catchup" },
  { id: "todo.close-hint", command: "closeHint" },
  ...move("catchup"),
  { id: "todo.catchup-keep", command: "keep" },
  { id: "todo.catchup-let-go", command: "letGo" },
  { id: "todo.catchup-let-go-all", command: "letGoAll" },
  { id: "todo.catchup-back", command: "back" },
  ...move("expired"),
  { id: "todo.restore", command: "restore" },
  { id: "todo.expired-back", command: "back" },
];
