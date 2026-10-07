import { describe, expect, test } from "vitest";

import {
  activeChip,
  amountCopy,
  amountUnchecked,
  groupByMonth,
  KIND_CHIPS,
  provenanceLine,
  referenceCopy,
  stepYear,
} from "./ledger";
import { monthFixture, recordFixture } from "./testing";

describe("Archive's ledger", () => {
  test("rows sit under the daemon's month headers, whose totals cover every matching record", () => {
    const march = monthFixture("2025-03", 3, "£1,412.40");
    const february = monthFixture("2025-02", 5, "£611.05");
    const groups = groupByMonth(
      [
        recordFixture({ id: "dell", date: "2025-03-03T12:00:00Z" }),
        recordFixture({ id: "octopus", date: "2025-03-11T12:00:00Z" }),
        recordFixture({ id: "feb", date: "2025-02-20T12:00:00Z" }),
      ],
      [march, february],
    );
    expect(groups.map((group) => [group.month?.label, group.records.map((r) => r.id)])).toEqual([
      ["2025 · March", ["dell", "octopus"]],
      ["2025 · February", ["feb"]],
    ]);
    // The header's count is the month's, even when this page holds two of three.
    expect(groups[0]?.month?.count).toBe(3);
  });

  test("an undated record gets its own group rather than joining a month", () => {
    const groups = groupByMonth([recordFixture({ date: null })], [monthFixture("2025-03", 1)]);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.month).toBeNull();
  });

  test("kind chips name what people call records, and a custom set picks none", () => {
    expect(KIND_CHIPS.map((chip) => chip.label)).toEqual([
      "All",
      "Receipts",
      "Orders",
      "Trips",
      "Bills",
      "Documents",
    ]);
    expect(activeChip({})).toBe("all");
    expect(activeChip({ kinds: ["statement", "invoice"] })).toBe("bills");
    expect(activeChip({ kinds: ["ticket"] })).toBeNull();
  });

  test("[ steps back a year among the years with records and ] steps forward to all years", () => {
    const years = [2026, 2025, 2023];
    expect(stepYear(years, undefined, -1)).toBe(2026);
    expect(stepYear(years, 2026, -1)).toBe(2025);
    expect(stepYear(years, 2025, -1)).toBe(2023);
    expect(stepYear(years, 2023, -1)).toBe(2023);
    expect(stepYear(years, 2023, 1)).toBe(2025);
    expect(stepYear(years, 2026, 1)).toBeNull();
    expect(stepYear([], undefined, -1)).toBeNull();
  });

  test("y copies the reference as written and Y the amount as a plain number", () => {
    const record = recordFixture();
    expect(referenceCopy(record)).toBe("K7QX2M");
    expect(amountCopy(record)).toBe("212.40");
    expect(amountCopy(recordFixture({ fields: [] }))).toBeNull();
  });

  test("an amount nobody checked is marked, and provenance says so", () => {
    expect(amountUnchecked(recordFixture({ unchecked_fields: ["amount"] }))).toBe(true);
    expect(amountUnchecked(recordFixture())).toBe(false);
    expect(provenanceLine("schema.org markup", true)).toBe("from schema.org markup · checked");
    expect(provenanceLine("a pattern in the email", false)).toBe(
      "from a pattern in the email · unchecked",
    );
  });
});
