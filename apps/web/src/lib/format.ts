const pluralRules = new Intl.PluralRules("en");

export function plural(count: number, one: string, many = `${one}s`): string {
  return `${count.toLocaleString()} ${pluralRules.select(count) === "one" ? one : many}`;
}

const timeFormats = new Map<string | undefined, Intl.DateTimeFormat>();

function timeFormatFor(locale: string | undefined): Intl.DateTimeFormat {
  let format = timeFormats.get(locale);
  if (!format) {
    const cycle = new Intl.DateTimeFormat(locale, { hour: "numeric" }).resolvedOptions().hourCycle;
    const twentyFourHour = cycle === "h23" || cycle === "h24";
    format = new Intl.DateTimeFormat(locale, {
      hour: twentyFourHour ? "2-digit" : "numeric",
      minute: "2-digit",
    });
    timeFormats.set(locale, format);
  }
  return format;
}
const weekdayFormat = new Intl.DateTimeFormat(undefined, { weekday: "short" });
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

export function formatTime(value: Date): string {
  return formatTimeIn(undefined, value);
}

export function formatTimeIn(locale: string | undefined, value: Date): string {
  return timeFormatFor(locale).format(value);
}

export function formatWhen(value: string | Date | null | undefined, now = new Date()): string {
  const date = parse(value);
  if (!date) return "";
  const seconds = Math.round((now.getTime() - date.getTime()) / 1000);
  if (Math.abs(seconds) < 60) return "Just now";
  const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
  if (days === 0)
    return seconds > 0 && seconds < 3_600 ? `${Math.floor(seconds / 60)}m` : formatTime(date);
  if (days === 1) return "Yesterday";
  if (days === -1) return "Tomorrow";
  if (Math.abs(days) < 7) return weekdayFormat.format(date);
  if (date.getFullYear() === now.getFullYear()) return dayMonthFormat.format(date);
  return fullDateFormat.format(date);
}

const receivedDayFormat = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  day: "numeric",
  month: "short",
});
const receivedDayYearFormat = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  day: "numeric",
  month: "short",
  year: "numeric",
});

export function formatReceived(value: string | Date | null | undefined, now = new Date()): string {
  const date = parse(value);
  if (!date) return "";
  const time = formatTime(date);
  if (startOfDay(date) === startOfDay(now)) return `Today, ${time} · ${formatRelative(date, now)}`;
  const day =
    date.getFullYear() === now.getFullYear()
      ? receivedDayFormat.format(date)
      : receivedDayYearFormat.format(date);
  return `${day}, ${time}`;
}

export function formatLongDate(value: string | Date | null | undefined): string {
  const date = parse(value);
  return date ? longFormat.format(date) : "";
}

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

export function parseAddress(value: string | null | undefined): ParsedAddress {
  if (!value) return { name: null, email: null };
  const angle = value.match(/^\s*"?([^"<]*?)"?\s*<([^>]+@[^>]+)>\s*$/);
  if (angle) return { name: angle[1]?.trim() || null, email: angle[2]?.trim() ?? null };
  const bare = value.match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/i);
  return bare ? { name: null, email: bare[0] } : { name: value.trim() || null, email: null };
}

export function initials(nameOrEmail: string): string {
  const cleaned = nameOrEmail.replace(/<.*>/, "").replace(/["']/g, "").trim();
  const words = cleaned.split(/[\s._-]+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1 || cleaned.includes("@")) return words[0]!.charAt(0).toUpperCase();
  return `${words[0]!.charAt(0)}${words.at(-1)!.charAt(0)}`.toUpperCase();
}
