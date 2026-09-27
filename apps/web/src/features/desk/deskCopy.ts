/*
 * The desk's words: the headline, the quiet line under it, and how each
 * row states its age. Pure functions of the daemon's desk and a clock, so
 * the copy is tested for plurals, empty states and the "no em dash" rule.
 */

import type { Desk, DeskLaneKind, DeskRow } from "./api";
import { formatLongDate, plural } from "@/lib/format";

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

export interface HeadlineCount {
  lane: DeskLaneKind;
  text: string;
}

export interface DeskHeadline {
  /** "Saturday morning." */
  lead: string;
  /** "3 replies", "2 promises": each links to its lane. Empty when nothing is owed. */
  counts: HeadlineCount[];
  /** Said instead of the counts when there are none. */
  calm: string | null;
  /** The quiet line under the headline, or null when there is nothing to add. */
  sub: string | null;
}

function partOfDay(now: Date): string {
  const hour = now.getHours();
  if (hour >= 5 && hour < 12) return "morning";
  if (hour >= 12 && hour < 17) return "afternoon";
  if (hour >= 17 && hour < 22) return "evening";
  return "night";
}

export function deskHeadline(desk: Desk, now = new Date()): DeskHeadline {
  const weekday = now.toLocaleDateString(undefined, { weekday: "long" });
  const lead = `${weekday} ${partOfDay(now)}.`;
  const counts: HeadlineCount[] = [];
  if (desk.owed.total > 0) {
    counts.push({ lane: "owed", text: plural(desk.owed.total, "reply", "replies") });
  }
  if (desk.due.total > 0) {
    counts.push({ lane: "due", text: plural(desk.due.total, "promise") });
  }
  const anythingElse = desk.waiting.total + desk.people_new.total > 0;
  const calm =
    counts.length > 0 ? null : anythingElse ? "No replies owed." : "Nothing needs you right now.";

  const sub: string[] = [];
  if (desk.waiting.total > 0) {
    sub.push(`Waiting on ${plural(desk.waiting.total, "reply", "replies")}.`);
  }
  if (desk.people_new.total > 0) {
    sub.push(`${plural(desk.people_new.total, "new message")} from people.`);
  } else if (desk.last_from_people_at) {
    sub.push(`Nothing new from people since ${sinceLabel(desk.last_from_people_at, now)}.`);
  }
  return { lead, counts, calm, sub: sub.length > 0 ? sub.join(" ") : null };
}

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

function dayDelta(date: Date, now: Date): number {
  return Math.round((startOfDay(date) - startOfDay(now)) / (DAY * 1000));
}

const clock = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" });

/** "18:40", "yesterday 18:40", "Thu 09:10", "12 Sep". */
function sinceLabel(value: string, now: Date): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "a while";
  const delta = dayDelta(date, now);
  if (delta === 0) return clock.format(date);
  if (delta === -1) return `yesterday ${clock.format(date)}`;
  if (delta > -7) {
    return `${date.toLocaleDateString(undefined, { weekday: "short" })} ${clock.format(date)}`;
  }
  return date.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** "12m", "5h", "3d", "2w": how long something has been going on. */
export function shortDuration(seconds: number): string {
  const s = Math.max(0, seconds);
  if (s < HOUR) return `${Math.max(1, Math.floor(s / MINUTE))}m`;
  if (s < DAY) return `${Math.floor(s / HOUR)}h`;
  if (s < 14 * DAY) return `${Math.floor(s / DAY)}d`;
  return `${Math.floor(s / (7 * DAY))}w`;
}

/**
 * A calendar-style point in time: "now", "12m", "5h" today, "yesterday",
 * "Thu" this week, "12 Sep" before that; "today", "tomorrow" and "Mon" for
 * dates ahead.
 */
export function whenLabel(value: string, now = new Date()): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const seconds = Math.round((now.getTime() - date.getTime()) / 1000);
  const delta = dayDelta(date, now);
  if (seconds >= 0) {
    if (seconds < MINUTE) return "now";
    if (delta === 0) return shortDuration(seconds);
    if (delta === -1) return "yesterday";
  } else {
    if (delta === 0) return "today";
    if (delta === 1) return "tomorrow";
  }
  if (Math.abs(delta) < 7) return date.toLocaleDateString(undefined, { weekday: "short" });
  const sameYear = date.getFullYear() === now.getFullYear();
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}

export interface RowAge {
  /** "2d", "yesterday", "Mon". */
  label: string;
  /** "usually 4h", when the person's pace is known. */
  usual: string | null;
  /** Past the person's usual pace, or past a promise's date. */
  late: boolean;
  /** Full date for a tooltip and screen readers. */
  title: string;
}

/**
 * How a row states its time. Waiting rows and rows with a known pace read
 * as a duration ("5d", "2d · usually 4h") because that is the comparison
 * that matters; promises read as their due day; everything else as when
 * it arrived.
 */
export function rowAge(row: DeskRow, now = new Date()): RowAge {
  const usual = row.usual_seconds ?? null;
  const since = row.since;
  const title = row.lane === "due" ? `Due ${formatLongDate(since)}` : formatLongDate(since);
  if (row.lane === "due") {
    return { label: whenLabel(since, now), usual: null, late: Boolean(row.overdue), title };
  }
  const asDuration = row.lane === "waiting" || usual !== null;
  return {
    label: asDuration ? shortDuration(row.age_seconds) : whenLabel(since, now),
    usual: usual !== null ? `usually ${shortDuration(usual)}` : null,
    late: Boolean(row.overdue),
    title,
  };
}

export const LANE_TITLES: Record<DeskLaneKind, string> = {
  owed: "You owe",
  due: "Due",
  waiting: "Waiting on",
  people_new: "New from people",
};

/** The person a row is about: promises are "to Nora", everything else just "Nora". */
export function rowPerson(row: DeskRow): string {
  const name = row.counterparty_name?.trim() || row.counterparty_email;
  return row.lane === "due" ? `to ${name}` : name;
}
