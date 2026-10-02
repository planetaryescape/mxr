import { describe, expect, test } from "vitest";

import type { OwedReplyRow } from "./api";
import { waitingLine } from "./waitingLine";

function row(waitingDays: number, usualSeconds?: number | null): OwedReplyRow {
  return {
    thread_id: "t",
    latest_inbound_msg_id: "m",
    from_email: "maya@example.com",
    subject: "Launch plan",
    latest_inbound_at: "2026-10-01T09:00:00Z",
    waiting_days: waitingDays,
    expected_days: 1,
    overdue_score: waitingDays,
    usual_seconds: usualSeconds,
  };
}

describe("waitingLine", () => {
  test("names your usual reply time only when history gives one", () => {
    expect(waitingLine(row(2, 4 * 3600))).toBe("Waiting 2d; you usually reply within 4h");
    expect(waitingLine(row(2))).toBe("Waiting 2d");
    expect(waitingLine(row(2, null))).toBe("Waiting 2d");
  });

  test("a wait under a day reads in hours", () => {
    expect(waitingLine(row(0.25))).toBe("Waiting 6h");
  });
});
