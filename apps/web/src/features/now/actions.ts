/*
 * Now's keys, from blueprint 22's key table and the daemon's Now guide.
 * The mounted Now page registers what each command does
 * (`useScopeController("now", …)`). Acting on a row does it in that row's
 * own mode: `e` is done here, through `SetModeDone`.
 */

import { Check, Reply } from "lucide-react";

import type { Action, CommandAction } from "@/lib/actions/types";

function key(
  id: string,
  command: string,
  label: string,
  shortcut: string,
  extra: Partial<Omit<CommandAction, "id" | "command" | "run">> = {},
): CommandAction {
  return { id, command, label, shortcut, group: "Triage", scopes: ["now"], ...extra };
}

export const nowActions: Action[] = [
  key("now.down", "down", "Next", "j", {
    group: "Move",
    aliases: ["ArrowDown"],
    hideInPalette: true,
  }),
  key("now.up", "up", "Previous", "k", {
    group: "Move",
    aliases: ["ArrowUp"],
    hideInPalette: true,
  }),
  key("now.open", "open", "Open in its mode", "Enter", { shortLabel: "Open", group: "Move" }),
  key("now.done", "done", "Done here", "e", { shortLabel: "Done here", icon: Check }),
  key("now.reply", "reply", "Reply", "r", { shortLabel: "Reply", icon: Reply }),
  key("now.open-email", "openEmail", "Open the email", "o", { shortLabel: "Email" }),
  key("now.let-go-digest", "letGoDigest", "Let go of the Updates digest…", "A", {
    shortLabel: "Let go of digest",
    tuiNote: "Previews the daemon's dry run first; undo afterwards",
  }),
  key("now.close-hint", "closeHint", "Dismiss the hint", "Escape", { hideInPalette: true }),
];
