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
  index: number;
}

export interface DigestRows {
  rows: DigestRow[];
  bySection: Record<UpdateSection, DigestRow[]>;
  routine: { sources: number; messages: number };
}

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

export function routineLine(routine: DigestRows["routine"]): string {
  return `${SECTION_TITLE.routine} · ${plural(routine.sources, "source")}`;
}

export function cutLine(digest: UpdatesDigest): string {
  return `${plural(digest.message_count, "update")} from ${plural(digest.source_count, "source")}`;
}

export function cutLabel(digest: UpdatesDigest): string {
  return `${digest.cut.label} digest`;
}

export function isParcel(line: UpdateLine): boolean {
  return line.tracker?.kind === "parcel";
}

export function canLetGoSource(line: UpdateLine): boolean {
  return !isParcel(line) && (line.message_ids?.length ?? 0) > 0;
}

export function canTune(line: UpdateLine): boolean {
  return !isParcel(line);
}

export function canSweepSource(line: UpdateLine): boolean {
  return !isParcel(line) && Boolean(line.sender_email);
}

export function canPinSource(line: UpdateLine): boolean {
  return Boolean(line.latest_message_id);
}

export function openableLink(line: UpdateLine): UpdateLink | undefined {
  if (line.signal === "needs_you" || line.section === "needs_a_look") return undefined;
  return line.link ?? undefined;
}

export type SuggestedSetting = Extract<UpdateSetting, "muted" | "changes_only">;

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

export function tuneToast(sourceName: string, setting: UpdateSetting): string {
  const effect: Record<UpdateSetting, string> = {
    every_digest: "in every digest",
    changes_only: "only when something changes",
    muted: "muted, its mail stays in Archive and search",
    breakthrough: "always first, with a suggested to-do",
  };
  return `${sourceName}: ${effect[setting]}.`;
}

export interface TrackState {
  steps: string[];
  step: number | null;
}

export function trackStep(line: UpdateLine): TrackState {
  const tracker = line.tracker;
  const steps = tracker?.steps;
  if (!Array.isArray(steps) || steps.length === 0) return { steps: [], step: null };
  return { steps, step: tracker?.step ?? null };
}

export function trackReplacesFact(line: UpdateLine, track: string | null): boolean {
  return (
    track !== null &&
    line.fact_source === "rule" &&
    line.section !== "needs_a_look" &&
    line.tracker?.outcome !== "quiet"
  );
}

export function trackLabel(line: UpdateLine): string | null {
  const { steps, step } = trackStep(line);
  if (step === null || !Number.isInteger(step) || step < 0 || step >= steps.length) return null;
  const next = steps[step + 1];
  if (next === undefined) return null;
  const here = steps[step] ?? "";
  return `${here.charAt(0).toUpperCase()}${here.slice(1)} · next: ${next}`;
}

export function gistThread(line: UpdateLine): string | null {
  if (line.fact_source !== "subject" || !line.latest_thread_id) return null;
  if (line.fact_message_id && line.fact_message_id !== line.latest_message_id) return null;
  return line.latest_thread_id;
}

function senderHost(line: UpdateLine): string | null {
  const host = line.sender_email?.split("@")[1]?.trim().toLowerCase();
  return host ? host : null;
}

export interface SourceName {
  name: string;
  host: string | null;
}

export function splitSourceName(line: UpdateLine): SourceName {
  const host = senderHost(line);
  if (host) {
    const suffix = ` (${host})`;
    if (line.source_name.toLowerCase().endsWith(suffix)) {
      return { name: line.source_name.slice(0, -suffix.length), host };
    }
  }
  return { name: line.source_name, host: null };
}

export function displaySourceName(name: string): string {
  if (name.length === 0 || name !== name.toLowerCase()) return name;
  if ([...name].length <= 4) return name.toUpperCase();
  return name.charAt(0).toUpperCase() + name.slice(1);
}

export function numberNotInFact(line: UpdateLine, shown = line.fact) {
  return (line.numbers ?? []).find((number) => !shown.includes(number.raw));
}

export type TrailingItem =
  | { kind: "in_todo"; text: string }
  | { kind: "todo_suggestion" }
  | { kind: "delta"; text: string; title: string }
  | { kind: "amount"; text: string }
  | { kind: "count"; text: string };

export function trailingItem(line: UpdateLine, shown = line.fact): TrailingItem | null {
  if (line.in_todo) return { kind: "in_todo", text: line.in_todo };
  if (line.todo_suggestion) return { kind: "todo_suggestion" };
  if (line.delta) {
    return {
      kind: "delta",
      text: line.delta.text,
      title: `Computed from ${line.delta.raw} and ${line.delta.previous_raw}`,
    };
  }
  const amount = numberNotInFact(line, shown);
  if (amount) return { kind: "amount", text: amount.raw };
  if (line.section === "routine" && line.count > 1) {
    return { kind: "count", text: plural(line.count, "email") };
  }
  return null;
}
