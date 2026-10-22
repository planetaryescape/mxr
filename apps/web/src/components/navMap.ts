/*
 * The map of the app (docs/blueprint/26, D133): the places every width can
 * reach. The rail and the phone's More sheet both read these lists, so a
 * destination can't exist in one and be missing from the other. Everything
 * here is fixed data: the daemon being unreachable must not move or remove
 * an item. Only labels, saved searches and counts come from the daemon.
 */

import {
  BarChart3,
  CalendarDays,
  Clock,
  FileText,
  History,
  Hourglass,
  Layers,
  ListChecks,
  MailX,
  Package,
  Reply,
  Search,
  Send,
  Settings,
  Shield,
  ShieldAlert,
  Star,
  Stethoscope,
  Timer,
  Trash2,
  Users,
  Workflow,
} from "lucide-react";
import type { ComponentType } from "react";

import { definedShortcut } from "@/lib/actions/definition";

export type NavIcon = ComponentType<{ className?: string }>;

export interface NavEntry {
  key: string;
  to: string;
  /** Query the place is defined by (Messages, waiting on them). */
  search?: { turn: "theirs" } | { account?: string };
  /** Says what a count covers when the page it opens shows less of it. */
  hint?: string;
  label: string;
  Icon: NavIcon;
  /** Only work carries a count: Now, Messages, To do and the Screener's waiting senders. */
  count?: number;
  /** A quiet count (what a mode holds), not work: muted, never a dot. */
  quietCount?: boolean;
  /** "Early version: …": the mode is built on an existing view. */
  early?: string;
  shortcut?: string;
  /** A label's own colour, a validated hex, for the square before its name. */
  color?: string;
}

export interface NavSection {
  id: string;
  title?: string;
  foldable: boolean;
  entries: NavEntry[];
}

export const MAIL_ENTRIES: NavEntry[] = [
  {
    key: "starred",
    to: "/m/starred",
    label: "Starred",
    Icon: Star,
    shortcut: definedShortcut("nav.starred"),
  },
  { key: "sent", to: "/m/sent", label: "Sent", Icon: Send, shortcut: definedShortcut("nav.sent") },
  {
    key: "drafts",
    to: "/drafts",
    label: "Drafts",
    Icon: FileText,
    shortcut: definedShortcut("nav.drafts"),
  },
  {
    key: "snoozed",
    to: "/snoozed",
    label: "Snoozed",
    Icon: Clock,
    shortcut: definedShortcut("nav.snoozed"),
  },
  {
    key: "reply-queue",
    to: "/reply-queue",
    label: "Reply later",
    Icon: Reply,
    shortcut: definedShortcut("nav.reply-queue"),
  },
  {
    key: "archive",
    to: "/m/archive",
    label: "All Mail",
    Icon: Layers,
    shortcut: definedShortcut("nav.archive"),
  },
  {
    key: "spam",
    to: "/m/spam",
    label: "Spam",
    Icon: ShieldAlert,
    shortcut: definedShortcut("nav.spam"),
  },
  {
    key: "trash",
    to: "/m/trash",
    label: "Trash",
    Icon: Trash2,
    shortcut: definedShortcut("nav.trash"),
  },
];

/** Waiting on and the Screener lead the views: the two that wait on someone. */
export const VIEW_ENTRIES: NavEntry[] = [
  {
    key: "waiting",
    to: "/messages",
    search: { turn: "theirs" },
    label: "Waiting on",
    Icon: Timer,
    shortcut: definedShortcut("nav.waiting"),
  },
  {
    key: "owed",
    to: "/owed",
    label: "Owed replies",
    Icon: Hourglass,
    shortcut: definedShortcut("nav.owed"),
  },
  {
    key: "screener",
    to: "/screener",
    label: "Screener",
    Icon: Shield,
    shortcut: definedShortcut("nav.screener"),
  },
  { key: "subscriptions", to: "/subscriptions", label: "Subscriptions", Icon: MailX },
  {
    key: "invites",
    to: "/invites",
    label: "Invites",
    Icon: CalendarDays,
    shortcut: definedShortcut("nav.invites"),
  },
  {
    key: "deliveries",
    to: "/deliveries",
    label: "Deliveries",
    Icon: Package,
    shortcut: definedShortcut("nav.deliveries"),
  },
];

export const TOOL_ENTRIES: NavEntry[] = [
  {
    key: "search",
    to: "/search",
    label: "Search",
    Icon: Search,
    shortcut: definedShortcut("nav.search-page"),
  },
  {
    key: "analytics",
    to: "/analytics",
    label: "Analytics",
    Icon: BarChart3,
    shortcut: definedShortcut("nav.analytics"),
  },
  {
    key: "rules",
    to: "/rules",
    label: "Rules",
    Icon: Workflow,
    shortcut: definedShortcut("nav.rules"),
  },
  {
    key: "accounts",
    to: "/accounts",
    label: "Accounts",
    Icon: Users,
    shortcut: definedShortcut("nav.accounts"),
  },
  {
    key: "activity",
    to: "/activity",
    label: "Activity",
    Icon: History,
    shortcut: definedShortcut("nav.activity"),
  },
  { key: "jobs", to: "/jobs", label: "Jobs", Icon: ListChecks },
  {
    key: "diagnostics",
    to: "/diagnostics",
    label: "Diagnostics",
    Icon: Stethoscope,
    shortcut: definedShortcut("nav.diagnostics"),
  },
  {
    key: "settings",
    to: "/settings/all",
    label: "Settings",
    Icon: Settings,
    shortcut: definedShortcut("nav.settings"),
  },
];

const under = (base: string, path: string) => path === base || path.startsWith(`${base}/`);

/**
 * Whether a nav entry is the page on screen. Messages and "Waiting on" share
 * a path, so the turn tells them apart.
 */
export function isEntryActive(path: string, lane: unknown, entry: NavEntry): boolean {
  const base = entry.to;
  if (base === "/settings/all") return under("/settings", path);
  const onPath = under(base, path);
  if (base === "/messages") {
    const entryTurn = entry.search && "turn" in entry.search ? entry.search.turn : null;
    return onPath && entryTurn === (lane === "theirs" ? "theirs" : null);
  }
  return onPath;
}

export type PhoneTab = "now" | "messages" | "todo" | "reading" | "more";

/**
 * Which phone tab lights for a page. Exactly one, always: the four modes
 * that have a tab own their pages and conversations; every other page lives
 * in the More sheet, so More is lit there (D133).
 */
export function phoneTabFor(path: string, lane?: unknown): PhoneTab {
  if (under("/now", path)) return "now";
  // "Waiting on" is Messages' path with the turn flipped; it is a view in the
  // sheet, so it lights More rather than the Messages tab.
  if (under("/messages", path)) return lane === "theirs" ? "more" : "messages";
  if (under("/todo", path)) return "todo";
  if (under("/reading", path)) return "reading";
  return "more";
}
