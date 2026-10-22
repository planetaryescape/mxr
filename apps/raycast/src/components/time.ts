import type { List } from "@raycast/api";

// Raycast's `date` accessory only says "1d", "2w"; mail wants the time it
// arrived. Formats follow the system locale and time zone.

const clock = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const weekday = new Intl.DateTimeFormat(undefined, { weekday: "short" });
const dayMonth = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const dayMonthYear = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
  year: "numeric",
});
const full = new Intl.DateTimeFormat(undefined, { dateStyle: "full", timeStyle: "short" });

const DAY_MS = 24 * 60 * 60 * 1000;

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

/**
 * "14:32" today, "Yesterday 14:32", "Tue 14:32" within the week,
 * "21 Oct 14:32" this year, "21 Oct 2025" before that. Works for dates
 * ahead too ("Tomorrow 09:00"), for what is coming up.
 */
export function when(value: string | Date, now = new Date()): string {
  const date = typeof value === "string" ? new Date(value) : value;
  if (Number.isNaN(date.getTime())) return "";
  const days = Math.round((startOfDay(date) - startOfDay(now)) / DAY_MS);
  const time = clock.format(date);
  if (days === 0) return time;
  if (days === -1) return `Yesterday ${time}`;
  if (days === 1) return `Tomorrow ${time}`;
  if (Math.abs(days) < 7) return `${weekday.format(date)} ${time}`;
  if (date.getFullYear() === now.getFullYear()) return `${dayMonth.format(date)} ${time}`;
  return dayMonthYear.format(date);
}

/** The full date, for a tooltip or a detail view. */
export function fullDate(value: string | Date): string {
  const date = typeof value === "string" ? new Date(value) : value;
  return Number.isNaN(date.getTime()) ? "" : full.format(date);
}

/** A list accessory with the arrival time; the tooltip adds `why` when given. */
export function arrival(value: string, why?: string | null): List.Item.Accessory {
  const tooltip = fullDate(value);
  return { text: when(value), tooltip: why ? `${why}\n${tooltip}` : tooltip };
}
