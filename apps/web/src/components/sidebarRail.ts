/*
 * The sidebar's first section is the rail, in the daemon's order: Now, the
 * five modes, then Inbox (`GetRail`). Until the rail loads, the same
 * entries show without counts, so the sidebar never jumps.
 */

import {
  Bell,
  FolderArchive,
  Inbox,
  ListTodo,
  MessagesSquare,
  Newspaper,
  Sun,
  type LucideIcon,
} from "lucide-react";

import { RAIL_PATHS, railCount, type Rail, type RailEntry } from "@/features/modes/rail";

export interface RailNavEntry {
  key: string;
  to: string;
  label: string;
  Icon: LucideIcon;
  shortcut: string;
  /** `home` (Now), `modes`, or `lens` (Inbox). */
  group: string;
  count?: number;
  /** A badge counts work (Now); other counts are quiet. */
  badge: boolean;
  /** "Early version: …", for the entry's marker and tooltip. */
  early?: string;
  /** What the mode is for, for the tooltip. */
  header?: string;
}

const ICONS: Record<string, LucideIcon> = {
  now: Sun,
  messages: MessagesSquare,
  todo: ListTodo,
  updates: Bell,
  reading: Newspaper,
  archive: FolderArchive,
  inbox: Inbox,
};

/** The rail before the daemon answers: same order, keys and paths. */
const FALLBACK: Pick<RailEntry, "id" | "name" | "key" | "group">[] = [
  { id: "now", name: "Now", key: "g h", group: "home" },
  { id: "messages", name: "Messages", key: "g m", group: "modes" },
  { id: "todo", name: "To do", key: "g x", group: "modes" },
  { id: "updates", name: "Updates", key: "g u", group: "modes" },
  { id: "reading", name: "Reading", key: "g r", group: "modes" },
  { id: "archive", name: "Archive", key: "g e", group: "modes" },
  { id: "inbox", name: "Inbox", key: "g i", group: "lens" },
];

export function railNavEntries(rail: Rail | undefined): RailNavEntry[] {
  const entries: Pick<RailEntry, "id" | "name" | "key" | "group">[] = rail?.entries ?? FALLBACK;
  return entries.map((entry) => {
    const full = entry as Partial<RailEntry>;
    const count = full.status ? railCount(full as RailEntry) : null;
    return {
      key: entry.id,
      to: RAIL_PATHS[entry.id] ?? "/now",
      label: entry.name,
      Icon: ICONS[entry.id] ?? Inbox,
      shortcut: entry.key,
      group: entry.group,
      count: count?.value,
      badge: count?.badge ?? false,
      early: full.status === "early" ? (full.early_note ?? "Early version") : undefined,
      header: full.header ?? undefined,
    };
  });
}
