import { describe, expect, test } from "vitest";

import type { Desk, DeskRow } from "./api";
import { deskGroups } from "./deskRows";
import type { PendingMailOp } from "@/features/mail-actions/pendingMailOps";

function row(lane: DeskRow["lane"], id: string): DeskRow {
  return {
    lane,
    account_id: "a",
    thread_id: `t-${id}`,
    message_id: id,
    message_ids: [`${id}-0`, id],
    counterparty_email: `${id}@example.com`,
    subject: `Subject ${id}`,
    reason: "wrote to you",
    since: "2026-09-26T08:00:00Z",
    age_seconds: 3600,
    usual_samples: 0,
    overdue: false,
    unread: false,
  };
}

function desk(): Desk {
  const owed = Array.from({ length: 7 }, (_, index) => row("owed", `o${index}`));
  return {
    kind: "Desk",
    owed: { rows: owed, total: 9 },
    due: { rows: [{ ...row("due", "d0"), commitment_id: "c1" }], total: 1 },
    waiting: { rows: [], total: 0 },
    people_new: { rows: [row("people_new", "p0")], total: 1 },
    elsewhere: { reading: 0, paper_trail: 0, deliveries: 0, invites: 0, screener: 0 },
    generated_at: "2026-09-26T10:00:00Z",
  };
}

function archive(...ids: string[]): PendingMailOp {
  return { id: "op", action: "archive", messageIds: new Set(ids) };
}

describe("desk groups", () => {
  test("an optimistic archive takes the row off, lowers the count and pulls the next row up", () => {
    const { groups } = deskGroups(desk(), [archive("o0-0", "o0")]);
    const owed = groups[0]!;
    expect(owed.count).toBe(8);
    expect(owed.rows.map((r) => r.id)).toEqual(["o1", "o2", "o3", "o4", "o5"]);
  });
});
