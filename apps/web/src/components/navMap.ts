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
import type { ArchiveSearch } from "@/features/archive/archiveSearch";

export type NavIcon = ComponentType<{ className?: string }>;

export interface NavEntry {
  key: string;
  to: string;
  search?: { turn: "theirs" } | { account?: string } | ArchiveSearch;
  hint?: string;
  label: string;
  Icon: NavIcon;
  count?: number;
  quietCount?: boolean;
  early?: string;
  shortcut?: string;
  color?: string;
  accountId?: string;
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
    shortcut: definedShortcut("shell.search-palette"),
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

export function phoneTabFor(path: string, lane?: unknown): PhoneTab {
  if (under("/now", path)) return "now";
  if (under("/messages", path)) return lane === "theirs" ? "more" : "messages";
  if (under("/todo", path)) return "todo";
  if (under("/reading", path)) return "reading";
  return "more";
}
