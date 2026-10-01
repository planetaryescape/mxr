import { describe, expect, test } from "vitest";

import type { Desk, DeskLane, DeskRow } from "./api";
import { formatTime } from "@/lib/format";

import { deskHeadline, rowAge, rowPerson, shortDuration } from "./deskCopy";

// Saturday 26 September 2026, 10:00 local time.
const NOW = new Date(2026, 8, 26, 10, 0, 0);

function lane(total: number): DeskLane {
  return { rows: [], total };
}

function desk(counts: Partial<Record<"owed" | "due" | "waiting" | "people_new", number>>): Desk {
  return {
    kind: "Desk",
    owed: lane(counts.owed ?? 0),
    due: lane(counts.due ?? 0),
    waiting: lane(counts.waiting ?? 0),
    people_new: lane(counts.people_new ?? 0),
    elsewhere: { reading: 0, paper_trail: 0, deliveries: 0, invites: 0, screener: 0 },
    generated_at: NOW.toISOString(),
  };
}

function row(patch: Partial<DeskRow>): DeskRow {
  return {
    lane: "owed",
    account_id: "a",
    thread_id: "t",
    message_id: "m",
    message_ids: ["m"],
    counterparty_email: "maya@example.com",
    counterparty_name: "Maya Ortiz",
    subject: "Launch checklist",
    reason: "replied to your message",
    since: new Date(NOW.getTime() - 2 * 86_400_000).toISOString(),
    age_seconds: 2 * 86_400,
    usual_samples: 0,
    overdue: false,
    unread: true,
    ...patch,
  };
}

describe("desk headline", () => {
  test("counts work with correct plurals and names the part of the day", () => {
    const headline = deskHeadline(desk({ owed: 3, due: 1, waiting: 1, people_new: 2 }), NOW);
    expect(headline.lead).toBe("Saturday morning.");
    expect(headline.counts.map((count) => count.text)).toEqual(["3 replies", "1 promise"]);
    expect(headline.calm).toBeNull();
    expect(headline.sub).toBe("Waiting on 1 reply. 2 new messages from people.");
  });

  test("a clear desk is calm and specific, never scolding", () => {
    const clear = deskHeadline(
      { ...desk({}), last_from_people_at: new Date(2026, 8, 25, 18, 40).toISOString() },
      NOW,
    );
    expect(clear.counts).toEqual([]);
    expect(clear.calm).toBe("Nothing needs you right now.");
    // The clock follows the viewer's locale (18:40 or 06:40 PM).
    expect(clear.sub).toMatch(/^Nothing new from people since yesterday \d{2}:40( PM)?\.$/);
    expect(deskHeadline(desk({ waiting: 2 }), NOW).calm).toBe("No replies owed.");
  });

  test("no copy uses an em dash", () => {
    const headline = deskHeadline(desk({ owed: 1, due: 2, waiting: 3, people_new: 4 }), NOW);
    const text = [headline.lead, ...headline.counts.map((c) => c.text), headline.sub].join(" ");
    expect(text).not.toContain("—");
  });
});

describe("row time", () => {
  test("a known pace reads as a duration against the usual, late when past it", () => {
    const age = rowAge(row({ usual_seconds: 4 * 3600, usual_samples: 5, overdue: true }), NOW);
    expect(age).toMatchObject({ label: "2d", usual: "usually 4h", late: true });
  });

  test("without a pace, arrival reads as a day; waiting always as a duration", () => {
    const thursday = new Date(2026, 8, 24).toLocaleDateString(undefined, { weekday: "short" });
    expect(rowAge(row({}), NOW).label).toBe(thursday);
    expect(rowAge(row({ lane: "waiting", age_seconds: 5 * 86_400 }), NOW).label).toBe("5d");
    const yesterday = new Date(2026, 8, 25, 21, 0).toISOString();
    expect(rowAge(row({ since: yesterday, age_seconds: 13 * 3600 }), NOW).label).toBe("Yesterday");
  });

  test("promises read as their due day and are late once it passes", () => {
    const due = row({
      lane: "due",
      since: new Date(2026, 8, 28, 9, 0).toISOString(),
      age_seconds: -2 * 86_400,
    });
    const monday = new Date(2026, 8, 28).toLocaleDateString(undefined, { weekday: "short" });
    expect(rowAge(due, NOW)).toMatchObject({ label: monday, late: false });
    expect(rowPerson(due)).toBe("to Maya Ortiz");
    const tomorrow = row({ lane: "due", since: new Date(2026, 8, 27, 9, 0).toISOString() });
    expect(rowAge(tomorrow, NOW).label).toBe("Tomorrow");
    const tonight = new Date(2026, 8, 26, 18, 0);
    expect(rowAge(row({ lane: "due", since: tonight.toISOString() }), NOW).label).toBe(
      formatTime(tonight),
    );
  });

  test("short durations step from minutes to weeks", () => {
    expect([30, 90 * 60, 3 * 86_400, 20 * 86_400].map(shortDuration)).toEqual([
      "1m",
      "1h",
      "3d",
      "2w",
    ]);
  });
});
