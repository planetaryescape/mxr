/*
 * How the digest is laid out: three sections in a fixed order, Routine
 * folded after a few sources, and the cursor's order through all of it.
 * Pure, so the route and its tests read the same rows.
 */

import { plural } from "@/lib/format";

import type { UpdateLine, UpdateSection, UpdateSetting, UpdatesDigest } from "./api";

/** Routine sources shown before "+ N quieter sources". */
export const ROUTINE_SHOWN = 4;

export const SECTION_TITLE: Record<UpdateSection, string> = {
  needs_a_look: "Needs a look",
  changed: "Changed",
  routine: "Routine",
};

export interface DigestRow {
  line: UpdateLine;
  section: UpdateSection;
  /** Position in cursor order, across sections. */
  index: number;
}

export interface DigestRows {
  rows: DigestRow[];
  bySection: Record<UpdateSection, DigestRow[]>;
  /** Routine lines folded away: sources and the messages they hold. */
  quieter: { sources: number; messages: number };
}

/** The rows on screen, minus lines being let go, with Routine folded. */
export function digestRows(
  digest: UpdatesDigest,
  options: { routineOpen: boolean; hidden?: ReadonlySet<string> },
): DigestRows {
  const hidden = options.hidden ?? new Set<string>();
  const visible = (lines: UpdateLine[]) => lines.filter((line) => !hidden.has(line.id));
  const routine = visible(digest.routine);
  const shownRoutine = options.routineOpen ? routine : routine.slice(0, ROUTINE_SHOWN);
  const folded = routine.slice(shownRoutine.length);
  const sections: [UpdateSection, UpdateLine[]][] = [
    ["needs_a_look", visible(digest.needs_a_look)],
    ["changed", visible(digest.changed)],
    ["routine", shownRoutine],
  ];
  const rows: DigestRow[] = [];
  const bySection: DigestRows["bySection"] = { needs_a_look: [], changed: [], routine: [] };
  for (const [section, lines] of sections) {
    for (const line of lines) {
      const row = { line, section, index: rows.length };
      rows.push(row);
      bySection[section].push(row);
    }
  }
  return {
    rows,
    bySection,
    quieter: {
      sources: folded.length,
      messages: folded.reduce((sum, line) => sum + line.count, 0),
    },
  };
}

/** "This morning's digest · 08:00 · 31 updates from 12 sources". */
export function cutLine(digest: UpdatesDigest): string {
  return [
    digest.cut.title,
    digest.cut.label,
    `${plural(digest.message_count, "update")} from ${plural(digest.source_count, "source")}`,
  ].join(" · ");
}

/** A parcel tracker stands for a delivery, not mail: it leaves on its own. */
export function isParcel(line: UpdateLine): boolean {
  return line.tracker?.kind === "parcel";
}

/** `e` lets go of a source's mail; a parcel has none to let go. */
export function canLetGoSource(line: UpdateLine): boolean {
  return !isParcel(line) && (line.message_ids?.length ?? 0) > 0;
}

/** `K` tunes a source of mail. */
export function canTune(line: UpdateLine): boolean {
  return !isParcel(line);
}

/**
 * `L` opens the line's one link. The daemon never sends one on a line
 * that needs you (a sign-in, a failed payment); checked again here so a
 * money or security link is never one key away.
 */
export function openableLink(line: UpdateLine): UpdateLine["link"] {
  if (line.signal === "needs_you" || line.section === "needs_a_look") return undefined;
  return line.link ?? undefined;
}

export const SETTING_LABEL: Record<UpdateSetting, string> = {
  every_digest: "In every digest",
  changes_only: "Only when something changes",
  muted: "Mute",
  breakthrough: "Straight to To do on arrival",
};

export const SETTINGS: readonly UpdateSetting[] = [
  "every_digest",
  "changes_only",
  "muted",
  "breakthrough",
];

/** "Strava: muted." for the toast, from the daemon's own copy when it has it. */
export function tuneToast(sourceName: string, setting: UpdateSetting): string {
  const effect: Record<UpdateSetting, string> = {
    every_digest: "in every digest",
    changes_only: "only when something changes",
    muted: "muted, its mail stays in Archive and search",
    breakthrough: "straight to To do on arrival",
  };
  return `${sourceName}: ${effect[setting]}.`;
}

/** Where a parcel stands on its track, 0 to 1, for the dots. */
export function trackStep(line: UpdateLine): { steps: string[]; step: number | null } {
  const tracker = line.tracker;
  if (!tracker?.steps?.length) return { steps: [], step: null };
  return { steps: tracker.steps, step: tracker.step ?? null };
}
