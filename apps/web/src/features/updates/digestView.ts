/*
 * How the digest is laid out: three sections in a fixed order, Routine
 * collapsed to one line until opened, and the cursor's order through all of
 * it. Pure, so the route and its tests read the same rows.
 */

import { plural } from "@/lib/format";

import type { UpdateLine, UpdateLink, UpdateSection, UpdateSetting, UpdatesDigest } from "./api";

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
  /**
   * Routine's totals, shown on its one collapsed line. The digest carries no
   * routine total of its own; the daemon's headline sums the same lines.
   */
  routine: { sources: number; messages: number };
}

/** The rows on screen, minus lines being let go. Routine has no rows until opened. */
export function digestRows(
  digest: UpdatesDigest,
  options: { routineOpen: boolean; hidden?: ReadonlySet<string> },
): DigestRows {
  const hidden = options.hidden ?? new Set<string>();
  const visible = (lines: UpdateLine[]) => lines.filter((line) => !hidden.has(line.id));
  const routine = visible(digest.routine);
  const sections: [UpdateSection, UpdateLine[]][] = [
    ["needs_a_look", visible(digest.needs_a_look)],
    ["changed", visible(digest.changed)],
    ["routine", options.routineOpen ? routine : []],
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
    routine: {
      sources: routine.length,
      messages: routine.reduce((sum, line) => sum + line.count, 0),
    },
  };
}

/** "Routine · 31 updates from 16 sources", the one line Routine collapses to. */
export function routineLine(routine: DigestRows["routine"]): string {
  return `${SECTION_TITLE.routine} · ${plural(routine.messages, "update")} from ${plural(routine.sources, "source")}`;
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

/** `S` sweeps the sender's mail out of Paper trail; a parcel has no sender's mail to sweep. */
export function canSweepSource(line: UpdateLine): boolean {
  return !isParcel(line) && Boolean(line.sender_email);
}

/** `p` pins the source's latest email, which a sweep of its sender leaves in place. */
export function canPinSource(line: UpdateLine): boolean {
  return Boolean(line.latest_message_id);
}

/**
 * `L` opens the line's one link. The daemon never sends one on a line
 * that needs you (a sign-in, a failed payment); checked again here so a
 * money or security link is never one key away.
 */
export function openableLink(line: UpdateLine): UpdateLink | undefined {
  if (line.signal === "needs_you" || line.section === "needs_a_look") return undefined;
  return line.link ?? undefined;
}

/** The two settings a source's "mute it?" suggestion offers. */
export type SuggestedSetting = Extract<UpdateSetting, "muted" | "changes_only">;

/**
 * "Build failed" for a build or incident tracker. The schema types `kind`
 * as a string, so a kind this client doesn't know shows its state alone
 * rather than borrowing another kind's noun.
 */
export function trackerStatus(line: UpdateLine): string | null {
  const tracker = line.tracker;
  if (!tracker) return null;
  switch (tracker.kind) {
    case "parcel":
      return null;
    case "build":
      return `Build ${tracker.state_label}`;
    case "incident":
      return `Incident ${tracker.state_label}`;
    default:
      return tracker.state_label;
  }
}

export const SETTING_LABEL: Record<UpdateSetting, string> = {
  every_digest: "In every digest",
  changes_only: "Only when something changes",
  muted: "Mute",
  breakthrough: "Always first, with a suggested to-do",
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
    breakthrough: "always first, with a suggested to-do",
  };
  return `${sourceName}: ${effect[setting]}.`;
}

/** Where a parcel stands on its track, 0 to 1, for the dots. */
export function trackStep(line: UpdateLine): { steps: string[]; step: number | null } {
  const tracker = line.tracker;
  const steps = tracker?.steps;
  // A payload whose steps aren't a list shows no track, rather than one read character by character.
  if (!Array.isArray(steps) || steps.length === 0) return { steps: [], step: null };
  return { steps, step: tracker?.step ?? null };
}

/**
 * A parcel's track as one muted line: where it is and what comes next, as
 * "Shipped · next: out for delivery". Nothing at the last step, since the
 * fact already says it arrived.
 */
export function trackLabel(line: UpdateLine): string | null {
  const { steps, step } = trackStep(line);
  // The daemon's step is an index it computed; a value outside the steps
  // (or not an integer) shows no track rather than a wrong one.
  if (step === null || !Number.isInteger(step) || step < 0 || step >= steps.length) return null;
  const next = steps[step + 1];
  if (next === undefined) return null;
  const here = steps[step] ?? "";
  return `${here.charAt(0).toUpperCase()}${here.slice(1)} · next: ${next}`;
}

/** The sending host the daemon may append to a Needs a look name, lowercased. */
function senderHost(line: UpdateLine): string | null {
  const host = line.sender_email?.split("@")[1]?.trim().toLowerCase();
  return host ? host : null;
}

/**
 * "Google (accounts.google.com)" comes from the daemon with the sending host
 * beside a display name anyone can choose (crates/updates/src/fact.rs,
 * branded_name). Split it so the name stays whole and the host can be muted.
 * A name without that exact suffix is shown as it came.
 */
export function splitSourceName(line: UpdateLine): { name: string; host: string | null } {
  const host = senderHost(line);
  if (host) {
    const suffix = ` (${host})`;
    if (line.source_name.toLowerCase().endsWith(suffix)) {
      return { name: line.source_name.slice(0, -suffix.length), host };
    }
  }
  return { name: line.source_name, host: null };
}

/**
 * The first amount the mail quoted that the fact doesn't already say: the
 * amount a "Your scheduled payment" line is missing.
 */
export function numberNotInFact(line: UpdateLine) {
  return (line.numbers ?? []).find((number) => !line.fact.includes(number.raw));
}

/** The one value a row's trailing slot can show, in precedence order. */
export type TrailingItem =
  | { kind: "in_todo"; text: string }
  | { kind: "todo_suggestion" }
  | { kind: "delta"; text: string; title: string }
  | { kind: "amount"; text: string }
  | { kind: "count"; text: string };

/**
 * Only one item trails a row: already in To do, then the to-do affordance,
 * then the computed delta, then the first amount the fact leaves out, then a
 * routine count with its unit. Anything else waits for the peek.
 */
export function trailingItem(line: UpdateLine): TrailingItem | null {
  if (line.in_todo) return { kind: "in_todo", text: line.in_todo };
  if (line.todo_suggestion) return { kind: "todo_suggestion" };
  if (line.delta) {
    return {
      kind: "delta",
      text: line.delta.text,
      title: `Computed from ${line.delta.raw} and ${line.delta.previous_raw}`,
    };
  }
  const amount = numberNotInFact(line);
  if (amount) return { kind: "amount", text: amount.raw };
  if (line.section === "routine" && line.count > 1) {
    return { kind: "count", text: plural(line.count, "email") };
  }
  return null;
}
