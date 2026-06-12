/*
 * Global navigation + system actions. Mirror the chords already in
 * lib/keymap.ts so PR #3 can swap keymap to consume `getShortcutMap()`
 * without changing user-visible bindings.
 *
 * `g a` keeps Archive (matches lib/keymap.ts:75); Analytics moves to `g y`
 * in PR #3 to resolve the collision documented in PARITY_MATRIX.md.
 */

import {
  Archive,
  BarChart3,
  Inbox,
  Mail,
  Search,
  Send,
  Settings as SettingsIcon,
  Shield,
  Star,
} from "lucide-react";

import { newMessageIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { useModals } from "@/state/modalStore";

import { getRuntimeNavigate } from "./runtime";
import type { Action } from "./types";

function go(to: string): () => void {
  return () => getRuntimeNavigate().navigate(to);
}

export const navigationActions: Action[] = [
  {
    id: "shell.command-palette",
    label: "Command palette",
    description: "Open the command palette",
    group: "Navigate",
    shortcut: "$mod+KeyK",
    run: () => useModals.getState().setCommandPaletteOpen(true),
  },
  {
    id: "shell.search-palette",
    label: "Search",
    description: "Open the search palette",
    group: "Search",
    icon: Search,
    shortcut: "/",
    aliases: ["Slash", "2", "Digit2"],
    run: () => useModals.getState().setSearchPaletteOpen(true),
  },
  {
    id: "shell.help",
    label: "Help",
    description: "Show the keyboard reference",
    group: "Navigate",
    shortcut: "Shift+Slash",
    run: () => {
      const modals = useModals.getState();
      modals.setHelpOpen(!modals.helpOpen);
    },
  },
  {
    id: "shell.compose",
    label: "Compose",
    description: "Start a new message",
    group: "Compose",
    icon: Mail,
    shortcut: "KeyC",
    run: () => useComposeUi.getState().openCompose(newMessageIntent(), "overlay"),
  },
  {
    id: "nav.inbox",
    label: "Go to Inbox",
    group: "Navigate",
    icon: Inbox,
    shortcut: "g i",
    aliases: ["1", "Digit1"],
    run: go("/m/inbox"),
  },
  {
    id: "nav.starred",
    label: "Go to Starred",
    group: "Navigate",
    icon: Star,
    shortcut: "g s",
    run: go("/m/starred"),
  },
  {
    id: "nav.drafts",
    label: "Go to Drafts",
    group: "Navigate",
    shortcut: "g d",
    run: go("/m/drafts"),
  },
  {
    id: "nav.sent",
    label: "Go to Sent",
    group: "Navigate",
    icon: Send,
    paletteOnly: true,
    run: go("/m/sent"),
  },
  {
    id: "nav.archive",
    label: "Go to All Mail",
    group: "Navigate",
    icon: Archive,
    shortcut: "g a",
    run: go("/m/archive"),
  },
  {
    id: "nav.trash",
    label: "Go to Trash",
    group: "Navigate",
    shortcut: "g t",
    run: go("/m/trash"),
  },
  {
    id: "nav.snoozed",
    label: "Go to Snoozed",
    group: "Navigate",
    shortcut: "g n",
    run: go("/m/snoozed"),
  },
  {
    id: "nav.reply-queue",
    label: "Go to Reply queue",
    group: "Navigate",
    shortcut: "g l",
    run: go("/reply-queue"),
  },
  {
    id: "nav.subscriptions",
    label: "Go to Subscriptions",
    group: "Navigate",
    shortcut: "g u",
    run: go("/subscriptions"),
  },
  {
    id: "nav.rules",
    label: "Go to Rules",
    group: "Navigate",
    shortcut: "g r",
    run: go("/rules"),
  },
  {
    id: "nav.analytics",
    label: "Analytics",
    group: "Analytics",
    icon: BarChart3,
    shortcut: "g y",
    aliases: ["3", "Digit3"],
    run: go("/analytics"),
  },
  {
    id: "nav.rules-numeric",
    label: "Rules (numeric)",
    group: "Navigate",
    aliases: ["4", "Digit4"],
    paletteOnly: true,
    run: go("/rules"),
  },
  {
    id: "nav.screener",
    label: "Screener",
    group: "Triage",
    icon: Shield,
    aliases: ["5", "Digit5"],
    run: go("/screener"),
  },
  {
    id: "nav.subscriptions-numeric",
    label: "Subscriptions (numeric)",
    group: "Navigate",
    aliases: ["6", "Digit6"],
    paletteOnly: true,
    run: go("/subscriptions"),
  },
  {
    id: "nav.reply-queue-numeric",
    label: "Reply queue (numeric)",
    group: "Navigate",
    aliases: ["7", "Digit7"],
    paletteOnly: true,
    run: go("/reply-queue"),
  },
  {
    id: "nav.accounts",
    label: "Accounts",
    group: "Accounts",
    aliases: ["8", "Digit8"],
    run: go("/accounts"),
  },
  {
    id: "nav.diagnostics",
    label: "Diagnostics",
    group: "Diagnostics",
    aliases: ["9", "Digit9"],
    run: go("/diagnostics"),
  },
  {
    id: "nav.settings",
    label: "Settings",
    group: "Settings",
    icon: SettingsIcon,
    aliases: ["0", "Digit0"],
    run: go("/settings/theme"),
  },
];
