/*
 * Updates' keys, from blueprint 22's key table and the daemon's Updates
 * guide. The mounted briefing registers what each command does
 * (`useScopeController("updates", …)`); the reader keeps its own keys
 * while it has focus. `g u` opens Updates from anywhere.
 */

import { BellOff, Check, ExternalLink, ListPlus, Mail } from "lucide-react";

import type { Action, CommandAction } from "@/lib/actions/types";

function key(
  id: string,
  command: string,
  label: string,
  shortcut: string,
  extra: Partial<Omit<CommandAction, "id" | "command" | "run">> = {},
): CommandAction {
  return { id, command, label, shortcut, group: "Triage", scopes: ["updates"], ...extra };
}

export const updatesActions: Action[] = [
  key("updates.down", "down", "Next", "j", {
    group: "Move",
    aliases: ["ArrowDown"],
    hideInPalette: true,
  }),
  key("updates.up", "up", "Previous", "k", {
    group: "Move",
    aliases: ["ArrowUp"],
    hideInPalette: true,
  }),
  key("updates.expand", "expand", "Show quieter sources", "Enter", {
    shortLabel: "Expand",
    group: "Move",
  }),
  key("updates.let-go-all", "letGoAll", "Let go of the digest…", "A", {
    shortLabel: "Let go of digest",
    icon: Check,
    tuiNote: "Previews the daemon's dry run first; undo afterwards",
  }),
  key("updates.let-go-source", "letGoSource", "Let go of this source", "e", {
    shortLabel: "Let go of source",
  }),
  key("updates.needs-me", "needsMe", "This needs me: make it a to-do", "t", {
    shortLabel: "Needs me",
    icon: ListPlus,
  }),
  key("updates.tune", "tune", "Tune this source…", "K", {
    shortLabel: "Tune",
    icon: BellOff,
  }),
  key("updates.link", "link", "Open the link", "L", { shortLabel: "Link", icon: ExternalLink }),
  key("updates.open-email", "openEmail", "Open the email", "o", {
    shortLabel: "Email",
    icon: Mail,
  }),
  key("updates.close-card", "closeCard", "Close the note about Updates", "Escape", {
    hideInPalette: true,
  }),
];
