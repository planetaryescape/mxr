/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import { parseSendLater } from "./sendLater";

// Thursday 2026-06-11 15:00 local time.
const NOW = new Date(2026, 5, 11, 15, 0, 0);

describe("parseSendLater", () => {
  test("relative minutes and hours", () => {
    expect(parseSendLater("in 20 minutes", NOW)?.at.getTime()).toBe(
      NOW.getTime() + 20 * 60_000,
    );
    expect(parseSendLater("in 2 hours", NOW)?.at.getTime()).toBe(
      NOW.getTime() + 2 * 3_600_000,
    );
    expect(parseSendLater("in 3 days", NOW)?.at.getTime()).toBe(
      NOW.getTime() + 3 * 86_400_000,
    );
  });

  test("tomorrow defaults to 9am", () => {
    const parsed = parseSendLater("tomorrow", NOW);
    expect(parsed?.at.getDate()).toBe(12);
    expect(parsed?.at.getHours()).toBe(9);
  });

  test("tomorrow with explicit time", () => {
    const parsed = parseSendLater("tomorrow at 14:30", NOW);
    expect(parsed?.at.getDate()).toBe(12);
    expect(parsed?.at.getHours()).toBe(14);
    expect(parsed?.at.getMinutes()).toBe(30);
  });

  test("day names pick the next occurrence", () => {
    // NOW is a Thursday; "monday" is 4 days out.
    const parsed = parseSendLater("monday", NOW);
    expect(parsed?.at.getDay()).toBe(1);
    expect(parsed?.at.getTime()).toBeGreaterThan(NOW.getTime());
    expect(parsed?.at.getHours()).toBe(9);
  });

  test("abbreviated day with am/pm time", () => {
    const parsed = parseSendLater("next tue 8am", NOW);
    expect(parsed?.at.getDay()).toBe(2);
    expect(parsed?.at.getHours()).toBe(8);
  });

  test("today's day name skips to next week", () => {
    const parsed = parseSendLater("thursday", NOW);
    expect(parsed?.at.getDay()).toBe(4);
    expect(parsed?.at.getDate()).toBe(18);
  });

  test("bare time already past rolls to tomorrow", () => {
    const parsed = parseSendLater("9am", NOW);
    expect(parsed?.at.getDate()).toBe(12);
    expect(parsed?.at.getHours()).toBe(9);
  });

  test("bare future time stays today", () => {
    const parsed = parseSendLater("17:45", NOW);
    expect(parsed?.at.getDate()).toBe(11);
    expect(parsed?.at.getHours()).toBe(17);
    expect(parsed?.at.getMinutes()).toBe(45);
  });

  test("12am and 12pm parse correctly", () => {
    expect(parseSendLater("tomorrow 12am", NOW)?.at.getHours()).toBe(0);
    expect(parseSendLater("tomorrow 12pm", NOW)?.at.getHours()).toBe(12);
  });

  test("garbage returns null", () => {
    expect(parseSendLater("whenever", NOW)).toBeNull();
    expect(parseSendLater("in -3 hours", NOW)).toBeNull();
    expect(parseSendLater("25:99", NOW)).toBeNull();
    expect(parseSendLater("", NOW)).toBeNull();
  });
});
