/*
 * Natural-language parsing for the send-later input. Deliberately small:
 * the handful of phrases people actually type, biased toward never
 * scheduling into the past. Returns null for anything it can't read —
 * callers fall back to showing the preset list.
 */

const DAY_NAMES = [
  "sunday",
  "monday",
  "tuesday",
  "wednesday",
  "thursday",
  "friday",
  "saturday",
] as const;

const DEFAULT_MORNING_HOUR = 9;

export interface SendLaterParse {
  at: Date;
  label: string;
}

export function parseSendLater(input: string, now: Date = new Date()): SendLaterParse | null {
  const text = input.trim().toLowerCase().replace(/\s+/g, " ");
  if (!text) return null;

  // "in 20 minutes" / "in 2 hours" / "in 3 days"
  const relative = text.match(/^in (\d+) ?(min|mins|minute|minutes|h|hr|hour|hours|d|day|days)$/);
  if (relative) {
    const amount = Number(relative[1]);
    if (!Number.isFinite(amount) || amount <= 0) return null;
    const unit = relative[2] ?? "";
    const ms = unit.startsWith("m")
      ? amount * 60_000
      : unit.startsWith("h")
        ? amount * 3_600_000
        : amount * 86_400_000;
    const at = new Date(now.getTime() + ms);
    return { at, label: formatWhen(at, now) };
  }

  // "tomorrow", "tomorrow 9am", "tomorrow at 14:30"
  const tomorrow = text.match(/^tomorrow( at)?( .+)?$/);
  if (tomorrow) {
    const time = parseTime(tomorrow[2]?.trim() ?? "") ?? {
      hours: DEFAULT_MORNING_HOUR,
      minutes: 0,
    };
    const at = atTime(addDays(now, 1), time.hours, time.minutes);
    return { at, label: formatWhen(at, now) };
  }

  // "tonight" — 8pm today, or null if already past
  if (text === "tonight") {
    const at = atTime(now, 20, 0);
    return at > now ? { at, label: formatWhen(at, now) } : null;
  }

  // "monday", "next tue", "friday 8am", "next monday at 16:00"
  const dayMatch = text.match(/^(next )?([a-z]+)( at)?( .+)?$/);
  if (dayMatch) {
    const dayIndex = DAY_NAMES.findIndex(
      (name) => name === dayMatch[2] || name.slice(0, 3) === dayMatch[2],
    );
    if (dayIndex >= 0) {
      const time = parseTime(dayMatch[4]?.trim() ?? "") ?? {
        hours: DEFAULT_MORNING_HOUR,
        minutes: 0,
      };
      let delta = (dayIndex - now.getDay() + 7) % 7;
      // Bare day names mean the next occurrence; "next" skips a week when
      // the name is today's.
      if (delta === 0) delta = 7;
      const at = atTime(addDays(now, delta), time.hours, time.minutes);
      return { at, label: formatWhen(at, now) };
    }
  }

  // Bare time today: "9am", "14:30", "5:15pm" — tomorrow if already past.
  const time = parseTime(text);
  if (time) {
    let at = atTime(now, time.hours, time.minutes);
    if (at <= now) at = addDays(at, 1);
    return { at, label: formatWhen(at, now) };
  }

  return null;
}

function parseTime(text: string): { hours: number; minutes: number } | null {
  if (!text) return null;
  const match = text.match(/^(\d{1,2})(?::(\d{2}))? ?(am|pm)?$/);
  if (!match) return null;
  let hours = Number(match[1]);
  const minutes = match[2] ? Number(match[2]) : 0;
  const meridiem = match[3];
  if (minutes > 59) return null;
  if (meridiem) {
    if (hours < 1 || hours > 12) return null;
    if (meridiem === "pm" && hours !== 12) hours += 12;
    if (meridiem === "am" && hours === 12) hours = 0;
  } else if (hours > 23) {
    return null;
  }
  return { hours, minutes };
}

function addDays(date: Date, days: number): Date {
  const next = new Date(date);
  next.setDate(next.getDate() + days);
  return next;
}

function atTime(date: Date, hours: number, minutes: number): Date {
  const next = new Date(date);
  next.setHours(hours, minutes, 0, 0);
  return next;
}

function formatWhen(at: Date, now: Date): string {
  const sameDay = at.toDateString() === now.toDateString();
  const tomorrow = at.toDateString() === addDays(now, 1).toDateString();
  const time = new Intl.DateTimeFormat(undefined, {
    hour: "numeric",
    minute: "2-digit",
  }).format(at);
  if (sameDay) return `today ${time}`;
  if (tomorrow) return `tomorrow ${time}`;
  const day = new Intl.DateTimeFormat(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
  }).format(at);
  return `${day}, ${time}`;
}
