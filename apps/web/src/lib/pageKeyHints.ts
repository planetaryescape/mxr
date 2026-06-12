/*
 * Page-internal vim-style keybindings. NOT global actions — these are handled
 * by per-page components (mailbox virtualizer, thread reader). They appear in
 * HelpDialog so users can discover them; the actual binding lives where it
 * belongs (the page).
 *
 * Replaces the per-route sections of the deleted lib/shortcutHints.ts.
 */

import type { ActionContext } from "./actions/types";

export interface PageHint {
  key: string;
  label: string;
}

export interface PageHintSection {
  title: string;
  hints: PageHint[];
}

const READER_HINTS: PageHint[] = [
  { key: "j/k", label: "Scroll" },
  { key: "h", label: "Mail list" },
  { key: "r", label: "Reply" },
  { key: "a", label: "Reply all" },
  { key: "f", label: "Forward" },
  { key: "F", label: "Full reader" },
  { key: "s", label: "Star" },
  { key: "m", label: "Read/unread" },
  { key: "l", label: "Labels" },
  { key: "L", label: "Context" },
  { key: "A", label: "Attachments" },
  { key: "y", label: "Summary" },
  { key: "p", label: "Sender" },
  { key: "e", label: "Archive" },
  { key: "[/]", label: "Archive & prev/next" },
  { key: "Z", label: "Snooze" },
  { key: "!", label: "Spam" },
  { key: "Del", label: "Trash" },
];

const SIDEBAR_HINTS: PageHint[] = [
  { key: "j/k", label: "Move" },
  { key: "l/o", label: "Open lens" },
  { key: "1-0", label: "Sidebar nav" },
];

const MAILBOX_HINTS: PageHint[] = [
  { key: "j/k", label: "Move" },
  { key: "gg/G", label: "Top/bottom" },
  { key: "o", label: "Open" },
  { key: "x", label: "Select" },
  { key: "*a/*n", label: "Select all/none" },
  { key: "h", label: "Sidebar" },
  { key: "e", label: "Archive" },
  { key: "s", label: "Star" },
  { key: "m", label: "Read/unread" },
];

const COMPOSE_HINTS: PageHint[] = [
  { key: "Esc", label: "Leave dialogs" },
  { key: "Tab", label: "Move fields" },
  { key: "⌘Enter", label: "Send from confirmation" },
  { key: "⌘;", label: "Insert snippet" },
];

export function pageHintsForRoute(ctx: ActionContext): PageHintSection[] {
  if (ctx.path.startsWith("/compose")) {
    return [{ title: "Compose shortcuts", hints: COMPOSE_HINTS }];
  }
  if (ctx.path.startsWith("/m/") && ctx.activePane === "reader") {
    return [{ title: "Reader shortcuts", hints: READER_HINTS }];
  }
  if (ctx.path.startsWith("/m/") && ctx.activePane === "sidebar") {
    return [{ title: "Sidebar shortcuts", hints: SIDEBAR_HINTS }];
  }
  if (ctx.path.startsWith("/m/")) {
    return [{ title: "Mailbox shortcuts", hints: MAILBOX_HINTS }];
  }
  return [];
}
