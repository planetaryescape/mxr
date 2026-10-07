/*
 * Motion, selection and view keys inside the sidebar, the mail list and
 * the reader. Bindings follow the TUI's panes; the mounted view registers
 * the commands (`useScopeController`), so these only fire where they make
 * sense and help lists exactly what the current pane supports.
 */

import { Maximize2, SquareStack } from "lucide-react";

import type { Action, ActionGroup, ActionScope } from "./types";

function key(
  scope: ActionScope,
  id: string,
  command: string,
  label: string,
  shortcut: string,
  group: ActionGroup,
  extra: Partial<Omit<Action, "id" | "command" | "run">> = {},
): Action {
  return {
    id,
    command,
    label,
    shortcut,
    group,
    scopes: [scope],
    hideInPalette: true,
    ...extra,
  } as Action;
}

export const sidebarActions: Action[] = [
  key("sidebar", "sidebar.down", "down", "Next item", "j", "Move", { aliases: ["ArrowDown"] }),
  key("sidebar", "sidebar.up", "up", "Previous item", "k", "Move", { aliases: ["ArrowUp"] }),
  key("sidebar", "sidebar.top", "top", "First item", "g g", "Move"),
  key("sidebar", "sidebar.bottom", "bottom", "Last item", "G", "Move"),
  key("sidebar", "sidebar.open", "open", "Open", "Enter", "Move", {
    aliases: ["o", "l", "ArrowRight"],
  }),
  key("sidebar", "sidebar.collapse", "collapse", "Collapse section", "[", "View"),
  key("sidebar", "sidebar.expand", "expand", "Expand section", "]", "View"),
];

export const listActions: Action[] = [
  key("list", "list.down", "down", "Next conversation", "j", "Move", {
    shortLabel: "Next",
    aliases: ["ArrowDown"],
  }),
  key("list", "list.up", "up", "Previous conversation", "k", "Move", {
    shortLabel: "Previous",
    aliases: ["ArrowUp"],
  }),
  key("list", "list.top", "top", "First conversation", "g g", "Move", {
    shortLabel: "First",
    aliases: ["Home"],
  }),
  key("list", "list.bottom", "bottom", "Last conversation", "G", "Move", {
    shortLabel: "Last",
    aliases: ["End"],
  }),
  key("list", "list.page-down", "pageDown", "Half page down", "Ctrl+d", "Move", {
    shortLabel: "Page down",
    aliases: ["PageDown"],
  }),
  key("list", "list.page-up", "pageUp", "Half page up", "Ctrl+u", "Move", {
    shortLabel: "Page up",
    aliases: ["PageUp"],
  }),
  key("list", "list.viewport-top", "viewportTop", "Top of screen", "H", "Move"),
  key("list", "list.viewport-middle", "viewportMiddle", "Middle of screen", "M", "Move", {
    tuiNote: "L (bottom of screen) opens links, as in the TUI's list pane",
  }),
  key("list", "list.open", "open", "Open conversation", "Enter", "Move", {
    shortLabel: "Open",
    aliases: ["o", "ArrowRight"],
  }),
  key("list", "list.sidebar", "focusSidebar", "Go to sidebar", "h", "Move", {
    aliases: ["ArrowLeft"],
  }),
  key("list", "list.row-action", "rowAction", "This list's row action", "w", "Mail", {
    shortLabel: "Row action",
    description: "Wake now in Snoozed, Done in the reply queue",
    tuiNote: "Web only",
  }),
  key("list", "list.toggle-select", "toggleSelect", "Select and move down", "x", "Select", {
    shortLabel: "Select",
  }),
  key("list", "list.visual", "visual", "Visual line mode", "V", "Select", {
    description: "Extend a selection with j and k",
  }),
  key("list", "list.select-all", "selectAll", "Select all", "* a", "Select", {
    aliases: ["Mod+a"],
  }),
  key("list", "list.select-none", "selectNone", "Select none", "* n", "Select"),
  key("list", "list.select-read", "selectRead", "Select read", "* r", "Select", {
    tuiNote: "Web only",
  }),
  key("list", "list.select-unread", "selectUnread", "Select unread", "* u", "Select", {
    tuiNote: "Web only",
  }),
  key("list", "list.select-starred", "selectStarred", "Select starred", "* s", "Select", {
    tuiNote: "Web only",
  }),
  key("list", "list.escape", "escape", "Clear selection, then close", "Escape", "Select", {
    shortLabel: "Clear",
  }),
  key("list", "list.filter", "filter", "Filter this list", "g f", "Search", {
    aliases: ["Ctrl+f"],
    hideInPalette: false,
    tuiNote: "TUI Ctrl-f; Ctrl+F stays the browser's Find off macOS, so g f works everywhere",
  }),
  {
    id: "list.toggle-threads",
    command: "toggleThreads",
    label: "Toggle threads / messages",
    description: "Group the list by conversation or show single messages",
    group: "View",
    icon: SquareStack,
    scopes: ["list", "reader"],
    paletteOnly: true,
  } as Action,
];

export const readerActions: Action[] = [
  key("reader", "reader.scroll-down", "scrollDown", "Scroll down", "j", "Read", {
    aliases: ["ArrowDown"],
  }),
  key("reader", "reader.scroll-up", "scrollUp", "Scroll up", "k", "Read", { aliases: ["ArrowUp"] }),
  key("reader", "reader.page-down", "pageDown", "Page down", "Ctrl+d", "Read", {
    aliases: ["PageDown", "Space"],
  }),
  key("reader", "reader.page-up", "pageUp", "Page up", "Ctrl+u", "Read", {
    aliases: ["PageUp", "Shift+Space"],
  }),
  key("reader", "reader.top", "top", "Top of thread", "g g", "Read", { aliases: ["Home"] }),
  key("reader", "reader.bottom", "bottom", "End of thread", "G", "Read", { aliases: ["End"] }),
  key("reader", "reader.next-message", "nextMessage", "Next message", "J", "Read"),
  key("reader", "reader.prev-message", "prevMessage", "Previous message", "K", "Read"),
  key(
    "reader",
    "reader.toggle-message",
    "toggleMessage",
    "Expand or collapse message",
    "o",
    "Read",
    {
      tuiNote: "The TUI shows every message, and its o opens the original, like O",
    },
  ),
  // ";" as in Gmail: X moves the email to another mode (blueprint 22).
  key("reader", "reader.expand-all", "expandAll", "Expand or collapse all", ";", "Read", {
    shortLabel: "Expand all",
    tuiNote: "Web only",
  }),
  key("reader", "reader.next-thread", "nextThread", "Next conversation", "n", "Move", {
    shortLabel: "Next",
    tuiNote: "The TUI steps results with n/N; the web steps conversations",
  }),
  key("reader", "reader.prev-thread", "prevThread", "Previous conversation", "N", "Move", {
    shortLabel: "Previous",
  }),
  key("reader", "reader.archive-next", "archiveNext", "Archive, open next", "]", "Mail", {
    shortLabel: "Archive, next",
    tuiNote: "Web only",
  }),
  key("reader", "reader.archive-prev", "archivePrev", "Archive, open previous", "[", "Mail", {
    shortLabel: "Archive, previous",
    tuiNote: "Web only",
  }),
  key("reader", "reader.list", "focusList", "Back to list", "h", "Move", {
    shortLabel: "List",
    aliases: ["ArrowLeft"],
  }),
  key("reader", "reader.close", "close", "Close conversation", "Escape", "Move", {
    shortLabel: "Close",
    aliases: ["q"],
  }),
  key("reader", "reader.view-reader", "viewReader", "Reader view, or back to plain", "R", "View", {
    shortLabel: "Reader view",
    description: "Cleaned text with quotes folded; press again for plain text, as in the TUI",
  }),
  key(
    "reader",
    "reader.view-html",
    "viewHtml",
    "Formatted (HTML) view, or back to plain",
    "H",
    "View",
    {
      description: "Press again for plain text, as in the TUI",
    },
  ),
  {
    id: "reader.view-plain",
    command: "viewPlain",
    label: "Plain text view",
    group: "View",
    scopes: ["reader"],
    paletteOnly: true,
  } as Action,
  key("reader", "reader.remote", "toggleRemote", "Load remote images", "M", "View"),
  key("reader", "reader.signature", "toggleSignature", "Show or hide signatures", "S", "View"),
  key("reader", "reader.quotes", "toggleQuotes", "Show or hide quoted text", "Q", "View", {
    tuiNote: "Web only",
  }),
  key("reader", "reader.headers", "headers", "Raw headers", "g H", "View", {
    tuiNote: "CLI `mxr cat --view headers`; not in the TUI",
    hideInPalette: false,
  }),
  key("reader", "reader.open-original", "openOriginal", "Open original in a new tab", "O", "View"),
  key("reader", "reader.summary", "summarize", "Summarize thread", "y", "Read", {
    hideInPalette: false,
  }),
  key("reader", "reader.fullscreen", "fullscreen", "Full-width reader", "F", "View", {
    icon: Maximize2,
    hideInPalette: false,
  }),
];
