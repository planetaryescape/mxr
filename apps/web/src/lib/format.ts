/*
 * Display formatting shared by every surface, so counts, dates and names
 * read the same in the list, the reader, toasts and dashboards.
 */

const pluralRules = new Intl.PluralRules("en");

/** "1 message", "3 messages". Pass `many` for irregular plurals. */
export function plural(count: number, one: string, many = `${one}s`): string {
  return `${count.toLocaleString()} ${pluralRules.select(count) === "one" ? one : many}`;
}

const timeFormat = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const dayMonthFormat = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });
const fullDateFormat = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "short",
  day: "numeric",
});
const longFormat = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  year: "numeric",
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
});

function parse(value: string | Date | null | undefined): Date | null {
  if (!value) return null;
  const date = value instanceof Date ? value : new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

export function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

/**
 * List-row date, mail-client style: time today, "Mon" this week, "Sep 3"
 * this year, "Sep 3, 2024" before that.
 */
export function formatListDate(value: string | Date | null | undefined, now = new Date()): string {
  const date = parse(value);
  if (!date) return "";
  const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
  if (days <= 0) return timeFormat.format(date);
  if (days === 1) return "Yesterday";
  if (days < 7) return date.toLocaleDateString(undefined, { weekday: "short" });
  if (date.getFullYear() === now.getFullYear()) return dayMonthFormat.format(date);
  return fullDateFormat.format(date);
}

/** Reader header date: "Sat, Sep 26, 2026, 10:11 PM". */
export function formatLongDate(value: string | Date | null | undefined): string {
  const date = parse(value);
  return date ? longFormat.format(date) : "";
}

/** "2 hours ago", "in 3 days". */
export function formatRelative(value: string | Date | null | undefined, now = new Date()): string {
  const date = parse(value);
  if (!date) return "";
  const seconds = Math.round((date.getTime() - now.getTime()) / 1000);
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ["year", 31_536_000],
    ["month", 2_592_000],
    ["week", 604_800],
    ["day", 86_400],
    ["hour", 3_600],
    ["minute", 60],
  ];
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size) return rtf.format(Math.round(seconds / size), unit);
  }
  return "just now";
}

export interface ParsedAddress {
  name: string | null;
  email: string | null;
}

/** Split "Ada Lovelace <ada@example.com>" into parts. */
export function parseAddress(value: string | null | undefined): ParsedAddress {
  if (!value) return { name: null, email: null };
  const angle = value.match(/^\s*"?([^"<]*?)"?\s*<([^>]+@[^>]+)>\s*$/);
  if (angle) return { name: angle[1]?.trim() || null, email: angle[2]?.trim() ?? null };
  const bare = value.match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/i);
  return bare ? { name: null, email: bare[0] } : { name: value.trim() || null, email: null };
}

/** Initials for an avatar: "Ada Lovelace" → "AL", "ada@x.com" → "A". */
export function initials(nameOrEmail: string): string {
  const cleaned = nameOrEmail.replace(/<.*>/, "").replace(/["']/g, "").trim();
  const words = cleaned.split(/[\s._-]+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1 || cleaned.includes("@")) return words[0]!.charAt(0).toUpperCase();
  return `${words[0]!.charAt(0)}${words.at(-1)!.charAt(0)}`.toUpperCase();
}
