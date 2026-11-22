import type { Desk, DeskLaneKind, DeskRow } from "./api";
import { formatLongDate, formatTime, formatWhen, plural, startOfDay } from "@/lib/format";

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

export interface HeadlineCount {
  lane: DeskLaneKind;
  text: string;
}

export interface DeskHeadline {
  lead: string;
  counts: HeadlineCount[];
  calm: string | null;
  sub: string | null;
}

export function partOfDay(now: Date): string {
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

function dayDelta(date: Date, now: Date): number {
  return Math.round((startOfDay(date) - startOfDay(now)) / (DAY * 1000));
}

function sinceLabel(value: string, now: Date): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "a while";
  const delta = dayDelta(date, now);
  if (delta === 0) return formatTime(date);
  if (delta === -1) return `yesterday ${formatTime(date)}`;
  if (delta > -7) {
    return `${date.toLocaleDateString(undefined, { weekday: "short" })} ${formatTime(date)}`;
  }
  return date.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

export function shortDuration(seconds: number): string {
  const s = Math.max(0, seconds);
  if (s < HOUR) return `${Math.max(1, Math.floor(s / MINUTE))}m`;
  if (s < DAY) return `${Math.floor(s / HOUR)}h`;
  if (s < 14 * DAY) return `${Math.floor(s / DAY)}d`;
  return `${Math.floor(s / (7 * DAY))}w`;
}

export interface RowAge {
  label: string;
  usual: string | null;
  late: boolean;
  title: string;
}

export function rowAge(row: DeskRow, now = new Date()): RowAge {
  const usual = row.usual_seconds ?? null;
  const since = row.since;
  const title = row.lane === "due" ? `Due ${formatLongDate(since)}` : formatLongDate(since);
  if (row.lane === "due") {
    return { label: formatWhen(since, now), usual: null, late: Boolean(row.overdue), title };
  }
  const asDuration = row.lane === "waiting" || usual !== null;
  return {
    label: asDuration ? shortDuration(row.age_seconds) : formatWhen(since, now),
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

export function rowPerson(row: DeskRow): string {
  const name = row.counterparty_name?.trim() || row.counterparty_email;
  return row.lane === "due" ? `to ${name}` : name;
}
