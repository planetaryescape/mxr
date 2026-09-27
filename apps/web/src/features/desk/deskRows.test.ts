import { describe, expect, test } from "vitest";

import type { Desk, DeskRow } from "./api";
import { DESK_LANE_CAP, deskGroups, partialLane } from "./deskRows";
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
  test("lanes cap on the desk, keep their totals, and link to the rest", () => {
    const { groups, index } = deskGroups(desk(), []);
    expect(groups.map((group) => group.id)).toEqual(["owed", "due", "people_new"]);
    const owed = groups[0]!;
    expect(owed.rows).toHaveLength(DESK_LANE_CAP);
    expect(owed.count).toBe(9);
    expect(owed.more).toEqual({ label: "Show all 9", href: "/desk?lane=owed" });
    // Rows act on the whole conversation.
    expect(owed.rows[0]).toMatchObject({ kind: "thread", message_ids: ["o0-0", "o0"] });
    expect(index.get("d0")?.commitment_id).toBe("c1");
  });

  test("an optimistic archive takes the row off, lowers the count and pulls the next row up", () => {
    const { groups } = deskGroups(desk(), [archive("o0-0", "o0")]);
    const owed = groups[0]!;
    expect(owed.count).toBe(8);
    expect(owed.rows.map((r) => r.id)).toEqual(["o1", "o2", "o3", "o4", "o5"]);
  });

  test("archiving a promised thread keeps the promise on the desk", () => {
    const { groups } = deskGroups(desk(), [archive("d0-0", "d0")]);
    expect(groups.find((group) => group.id === "due")?.rows).toHaveLength(1);
  });

  test("a waiting row leaves at once on snooze, and while done-waiting is in flight", () => {
    const withWaiting: Desk = {
      ...desk(),
      waiting: { rows: [row("waiting", "w0")], total: 1 },
    };
    const snoozed = deskGroups(withWaiting, [
      { id: "op", action: "snooze", messageIds: new Set(["w0-0", "w0"]) },
    ]);
    expect(snoozed.groups.find((group) => group.id === "waiting")).toBeUndefined();
    // A move does not take a thread you started off Waiting on in the daemon,
    // so the desk does not pretend it did.
    const moved = deskGroups(withWaiting, [
      { id: "op", action: "move", messageIds: new Set(["w0-0", "w0"]), payload: { label: "x" } },
    ]);
    expect(moved.groups.find((group) => group.id === "waiting")?.rows).toHaveLength(1);
    const hidden = deskGroups(withWaiting, [], undefined, new Set(["t-w0"]));
    expect(hidden.groups.find((group) => group.id === "waiting")).toBeUndefined();
  });

  test("a lane page shows one lane in full", () => {
    const { groups } = deskGroups(desk(), [], "owed");
    expect(groups).toHaveLength(1);
    expect(groups[0]!.rows).toHaveLength(7);
    expect(groups[0]!.more).toBeUndefined();
  });

  test("a lane page knows when the daemon sent only part of the lane", () => {
    expect(partialLane(desk(), "owed")).toEqual({ shown: 7, total: 9 });
    expect(partialLane(desk(), "due")).toBeNull();
    expect(partialLane(desk(), undefined)).toBeNull();
  });
});
