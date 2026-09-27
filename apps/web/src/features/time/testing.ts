/*
 * Test double for the daemon's ResolveTime, for component tests that can't
 * reach a daemon. It covers only the phrases the tests type: offsets
 * ("in 2 hours", "in 3 days"), the ambiguous "fri 3", and fixed day phrases.
 * The real grammar is tested in Rust (mxr_core::natural_time).
 */

import type { ResolvedTime, TimeChoice } from "./api";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;
const UNIT_MS: Record<string, number> = {
  m: 60_000,
  min: 60_000,
  minutes: 60_000,
  h: HOUR,
  hour: HOUR,
  hours: HOUR,
  d: DAY,
  day: DAY,
  days: DAY,
  w: 7 * DAY,
  week: 7 * DAY,
  weeks: 7 * DAY,
};

export function fakeChoice(
  at: Date,
  overrides: Partial<TimeChoice> = {},
  now: Date = new Date(),
): TimeChoice {
  const time = `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
  return {
    at: at.toISOString(),
    local: at.toISOString(),
    label: time,
    date_label: at.toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" }),
    time_label: time,
    relative_label: relativeLabel(at.getTime() - now.getTime()),
    implied: [],
    ...overrides,
  };
}

export function fakeResolvedTime(input: string, now: Date = new Date()): ResolvedTime {
  const text = input.trim().toLowerCase();
  const end = input.trimEnd().length;
  const start = input.length - input.trimStart().length;
  const understood = (choices: TimeChoice[]): ResolvedTime => ({
    kind: "ResolvedTime",
    input,
    resolution: {
      input,
      at: choices[0]?.at ?? now.toISOString(),
      description: `${choices[0]?.date_label}, ${choices[0]?.time_label}`,
      spans: [{ start, end }],
      choices,
    },
    error: null,
  });

  const offset = text.match(/^(?:in )?(\d+) ?([a-z]+)$/);
  const unit = offset ? UNIT_MS[offset[2] ?? ""] : undefined;
  if (offset && unit) {
    return understood([fakeChoice(new Date(now.getTime() + Number(offset[1]) * unit), {}, now)]);
  }
  if (text === "fri 3") {
    const friday = nextWeekday(now, 5);
    return understood([
      fakeChoice(atHour(friday, 15), { label: "15:00", implied: ["meridiem"] }, now),
      fakeChoice(atHour(friday, 3), { label: "03:00", implied: ["meridiem"] }, now),
    ]);
  }
  if (/^(tomorrow|monday|next monday)( 9am)?$/.test(text)) {
    const day = text.includes("monday") ? nextWeekday(now, 1) : addDays(now, 1);
    return understood([fakeChoice(atHour(day, 9), {}, now)]);
  }
  const token = text.split(/\s+/).find((word) => word && !/^(fri|at|on)$/.test(word)) ?? text;
  return {
    kind: "ResolvedTime",
    input,
    resolution: null,
    error: {
      kind: "unrecognized",
      message: `Didn't catch "${token}". Try "fri 3pm" or "in 2d".`,
      token,
      understood: [],
    },
  };
}

function relativeLabel(ms: number): string {
  if (ms < DAY) return `in ${Math.round(ms / HOUR)} hours`;
  return `in ${Math.round(ms / DAY)} days`;
}

function addDays(date: Date, days: number): Date {
  const next = new Date(date);
  next.setDate(next.getDate() + days);
  return next;
}

function atHour(date: Date, hour: number): Date {
  const next = new Date(date);
  next.setHours(hour, 0, 0, 0);
  return next;
}

function nextWeekday(now: Date, weekday: number): Date {
  const delta = (weekday - now.getDay() + 7) % 7 || 7;
  return addDays(now, delta);
}
