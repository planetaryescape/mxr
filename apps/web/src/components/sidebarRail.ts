import { Inbox, type LucideIcon } from "lucide-react";

import { MODE_GLYPHS, type ModeGlyphId } from "@/components/ModeMark";
import { RAIL_PATHS, railCount, type Rail, type RailEntry } from "@/features/modes/rail";
import { definedShortcut } from "@/lib/actions/definition";

export interface RailNavEntry {
  key: string;
  to: string;
  label: string;
  Icon: LucideIcon;
  shortcut: string;
  group: string;
  count?: number;
  badge: boolean;
  early?: string;
  header?: string;
}

const FALLBACK: Pick<RailEntry, "id" | "name" | "key" | "group">[] = [
  { id: "now", name: "Now", key: definedShortcut("nav.now"), group: "home" },
  { id: "messages", name: "Messages", key: definedShortcut("nav.messages"), group: "modes" },
  { id: "todo", name: "To do", key: definedShortcut("nav.todo"), group: "modes" },
  { id: "updates", name: "Updates", key: definedShortcut("nav.updates"), group: "modes" },
  { id: "reading", name: "Reading", key: definedShortcut("nav.reading"), group: "modes" },
  { id: "archive", name: "Archive", key: definedShortcut("nav.archive-mode"), group: "modes" },
  { id: "inbox", name: "Inbox", key: definedShortcut("nav.inbox"), group: "lens" },
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
      Icon: MODE_GLYPHS[entry.id as ModeGlyphId] ?? Inbox,
      shortcut: entry.key,
      group: entry.group,
      count: count?.value,
      badge: count?.badge ?? false,
      early: full.status === "early" ? (full.early_note ?? "Early version") : undefined,
      header: full.header ?? undefined,
    };
  });
}
