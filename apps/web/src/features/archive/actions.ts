/*
 * Archive's keys, as in the TUI's records lens and blueprint 22's key
 * table. The mounted Archive registers what each command does
 * (`useScopeController`); the reader keeps its own keys while it has
 * focus. `g e` opens Archive from anywhere, and `T` on a conversation
 * (in verbActions) files it.
 */

import { BadgeCheck, Copy, Download, FileText, Filter, Mail, PenLine, X } from "lucide-react";

import type { Action, CommandAction } from "@/lib/actions/types";

function key(
  id: string,
  command: string,
  label: string,
  shortcut: string,
  extra: Partial<Omit<CommandAction, "id" | "command" | "run">> = {},
): CommandAction {
  return { id, command, label, shortcut, group: "Triage", scopes: ["archive"], ...extra };
}

export const archiveActions: Action[] = [
  key("archive.down", "down", "Next record", "j", {
    group: "Move",
    aliases: ["ArrowDown"],
    hideInPalette: true,
  }),
  key("archive.up", "up", "Previous record", "k", {
    group: "Move",
    aliases: ["ArrowUp"],
    hideInPalette: true,
  }),
  key("archive.ask", "ask", "Ask Archive", "/", {
    shortLabel: "Ask",
    description: "Type what you remember; the answer is the field you asked for",
  }),
  key("archive.copy-reference", "copyReference", "Copy the reference", "y", {
    shortLabel: "Copy ref",
    icon: Copy,
  }),
  key("archive.copy-amount", "copyAmount", "Copy the amount", "Y", {
    shortLabel: "Copy amount",
    icon: Copy,
  }),
  key("archive.open-document", "openDocument", "Open the document", "Enter", {
    shortLabel: "Document",
    icon: FileText,
  }),
  key("archive.open-email", "openEmail", "Open the email", "o", {
    shortLabel: "Email",
    icon: Mail,
    aliases: ["e"],
    tuiNote: "Archive has no done: records stay, so e opens the email too",
  }),
  key("archive.issuer", "issuer", "This issuer's records", "p", { shortLabel: "Issuer" }),
  key("archive.prev-year", "prevYear", "Previous year", "[", { group: "Move" }),
  key("archive.next-year", "nextYear", "Next year", "]", { group: "Move" }),
  key("archive.edit", "edit", "Fix a field…", ",", { shortLabel: "Fix", icon: PenLine }),
  key("archive.check", "check", "Mark checked", "v", { shortLabel: "Checked", icon: BadgeCheck }),
  key("archive.dismiss", "dismiss", "Not a record", "X", { shortLabel: "Not a record", icon: X }),
  key("archive.export", "export", "Export…", "E", {
    shortLabel: "Export",
    icon: Download,
    description: "CSV of the records in view, after a preview of what it holds",
  }),
  key("archive.make-todo", "makeTodo", "Make a to-do from this record", "t", {
    shortLabel: "To-do",
  }),
  key("archive.filter", "filter", "Filters…", "g f", { shortLabel: "Filters", icon: Filter }),
  key("archive.close", "close", "Close", "Escape", { hideInPalette: true }),
];
