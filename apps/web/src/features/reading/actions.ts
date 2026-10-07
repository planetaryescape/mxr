/*
 * Reading's keys, per blueprint 22's one key map: Enter read, L the linked
 * article, b later, e let go, D unsubscribe, R the sender's layout, A let
 * go of everything shown. The mounted edition and reader register what each
 * command does (`useScopeController`); `g r` opens Reading from anywhere.
 */

import { Bookmark, Check, ExternalLink, Highlighter, Layout, MailX } from "lucide-react";

import type { Action, ActionScope, CommandAction } from "@/lib/actions/types";

function key(
  scope: ActionScope,
  id: string,
  command: string,
  label: string,
  shortcut: string,
  extra: Partial<Omit<CommandAction, "id" | "command" | "run">> = {},
): CommandAction {
  return { id, command, label, shortcut, group: "Read", scopes: [scope], ...extra };
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

export const readingActions: Action[] = [
  ...move("reading", "reading"),
  key("reading", "reading.read", "read", "Read", "Enter", { shortLabel: "Read" }),
  key("reading", "reading.article", "article", "Fetch the linked article", "L", {
    shortLabel: "Article",
    icon: ExternalLink,
    description: "Contacts the article's site, named before anything is sent",
  }),
  key("reading", "reading.later", "later", "Save for later", "b", {
    shortLabel: "Later",
    icon: Bookmark,
  }),
  key("reading", "reading.let-go", "letGo", "Let go", "e", { shortLabel: "Let go", icon: Check }),
  key("reading", "reading.unsubscribe", "unsubscribe", "Unsubscribe…", "D", {
    shortLabel: "Unsubscribe",
    icon: MailX,
    tuiNote: "Shows the evidence and the method first; nothing is sent until you confirm",
  }),
  key("reading", "reading.original", "original", "Read in the sender's layout", "R", {
    shortLabel: "Original",
    icon: Layout,
  }),
  key("reading", "reading.let-go-all", "letGoAll", "Let go of everything shown…", "A", {
    shortLabel: "Let go of all",
    tuiNote: "Previews the daemon's dry run first; undo afterwards",
  }),
  key("reading", "reading.open-email", "openEmail", "Open the email as sent", "o", {
    shortLabel: "Email",
  }),
  key("reading", "reading.move-sender", "moveSender", "This sender here: move to…", "K", {
    description:
      "People, Reading, Paper trail, Screened out or automatic, for their future mail too",
  }),
  key("reading", "reading.later-shelf", "laterShelf", "Open Later", "B", {
    shortLabel: "Later shelf",
  }),
  key("reading", "reading.close-card", "closeCard", "Close the note about Reading", "Escape", {
    hideInPalette: true,
  }),
  // The reader.
  key("reading-reader", "reading.reader-down", "down", "Scroll down", "j", {
    group: "Move",
    aliases: ["ArrowDown"],
    hideInPalette: true,
  }),
  key("reading-reader", "reading.reader-up", "up", "Scroll up", "k", {
    group: "Move",
    aliases: ["ArrowUp"],
    hideInPalette: true,
  }),
  key("reading-reader", "reading.reader-article", "article", "Article: fetch and read it", "L", {
    shortLabel: "Article",
    icon: ExternalLink,
  }),
  key("reading-reader", "reading.reader-issue", "issue", "Issue: the email itself", "I", {
    shortLabel: "Issue",
  }),
  key("reading-reader", "reading.reader-later", "later", "Save for later", "b", {
    shortLabel: "Later",
    icon: Bookmark,
  }),
  key("reading-reader", "reading.reader-let-go", "letGo", "Let go and go back", "e", {
    shortLabel: "Let go",
    icon: Check,
  }),
  key("reading-reader", "reading.reader-next", "next", "Next item", "n", { shortLabel: "Next" }),
  key("reading-reader", "reading.reader-original", "original", "Sender's layout or cleaned", "R", {
    shortLabel: "Original",
    icon: Layout,
  }),
  key("reading-reader", "reading.reader-highlight", "highlight", "Highlight the selection", "h", {
    shortLabel: "Highlight",
    icon: Highlighter,
  }),
  key("reading-reader", "reading.reader-unsubscribe", "unsubscribe", "Unsubscribe…", "D", {
    shortLabel: "Unsubscribe",
    icon: MailX,
  }),
  key("reading-reader", "reading.reader-open-email", "openEmail", "Open the email as sent", "o", {
    shortLabel: "Email",
  }),
  key("reading-reader", "reading.reader-back", "back", "Back to the edition", "Escape", {
    hideInPalette: true,
  }),
];
