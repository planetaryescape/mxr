import { describe, expect, test } from "vitest";

import { formatTime, formatWhen } from "./format";

// Saturday 26 September 2026, 15:00 local time.
const NOW = new Date(2026, 8, 26, 15, 0, 0);
const at = (...parts: [number, number, number, number?, number?, number?]) => new Date(...parts);
const weekday = (date: Date) => date.toLocaleDateString(undefined, { weekday: "short" });
const dayMonth = (date: Date) =>
  new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }).format(date);
const fullDate = (date: Date) =>
  new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric" }).format(
    date,
  );

describe("formatWhen: one wording for an age, on every surface", () => {
  test("within a minute either way is just now", () => {
    expect(formatWhen(at(2026, 8, 26, 15, 0, 0), NOW)).toBe("Just now");
    expect(formatWhen(at(2026, 8, 26, 14, 59, 1), NOW)).toBe("Just now");
    expect(formatWhen(at(2026, 8, 26, 15, 0, 59), NOW)).toBe("Just now");
  });

  test("minutes this hour, then the time of day", () => {
    expect(formatWhen(at(2026, 8, 26, 14, 59, 0), NOW)).toBe("1m");
    expect(formatWhen(at(2026, 8, 26, 14, 0, 1), NOW)).toBe("59m");
    const hourAgo = at(2026, 8, 26, 14, 0, 0);
    expect(formatWhen(hourAgo, NOW)).toBe(formatTime(hourAgo));
    const morning = at(2026, 8, 26, 0, 5);
    expect(formatWhen(morning, NOW)).toBe(formatTime(morning));
  });

  test("yesterday by calendar day, not by 24 hours", () => {
    expect(formatWhen(at(2026, 8, 25, 23, 59), NOW)).toBe("Yesterday");
    expect(formatWhen(at(2026, 8, 25, 0, 1), NOW)).toBe("Yesterday");
  });

  test("the weekday within the week, then the date", () => {
    const monday = at(2026, 8, 21, 9);
    expect(formatWhen(monday, NOW)).toBe(weekday(monday));
    const sixDays = at(2026, 8, 20, 9);
    expect(formatWhen(sixDays, NOW)).toBe(weekday(sixDays));
    const aWeek = at(2026, 8, 19, 9);
    expect(formatWhen(aWeek, NOW)).toBe(dayMonth(aWeek));
    const january = at(2026, 0, 2, 9);
    expect(formatWhen(january, NOW)).toBe(dayMonth(january));
  });

  test("another year carries the year", () => {
    const lastYear = at(2025, 11, 31, 9);
    expect(formatWhen(lastYear, NOW)).toBe(fullDate(lastYear));
  });

  test("future-dated: time today, tomorrow, the weekday, then the date", () => {
    const tonight = at(2026, 8, 26, 21);
    expect(formatWhen(tonight, NOW)).toBe(formatTime(tonight));
    expect(formatWhen(at(2026, 8, 27, 9), NOW)).toBe("Tomorrow");
    const thursday = at(2026, 9, 1, 9);
    expect(formatWhen(thursday, NOW)).toBe(weekday(thursday));
    const later = at(2026, 9, 20, 9);
    expect(formatWhen(later, NOW)).toBe(dayMonth(later));
    const nextYear = at(2027, 0, 4, 9);
    expect(formatWhen(nextYear, NOW)).toBe(fullDate(nextYear));
  });

  test("an empty or unreadable value is blank", () => {
    expect(formatWhen(null, NOW)).toBe("");
    expect(formatWhen("not a date", NOW)).toBe("");
  });
});
