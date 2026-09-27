import { describe, expect, test } from "vitest";

import { screenerEntry } from "./Sidebar";
import type { Desk } from "@/features/desk/api";

function desk(accountId: string | null, screener: number, screenerAccount?: string): Desk {
  const lane = { rows: [], total: 0 };
  return {
    kind: "Desk",
    account_id: accountId,
    owed: lane,
    due: lane,
    waiting: lane,
    people_new: lane,
    elsewhere: {
      reading: 0,
      paper_trail: 0,
      deliveries: 0,
      invites: 0,
      screener,
      screener_account: screenerAccount,
    },
    generated_at: "2026-09-27T10:00:00Z",
  };
}

describe("sidebar Screener", () => {
  test("opens the first account with senders, and says a summed count is a sum", () => {
    const entry = screenerEntry(desk(null, 5, "acct-2"));
    expect(entry).toMatchObject({ to: "/screener", count: 5, search: { account: "acct-2" } });
    expect(entry?.hint).toBe(
      "5 new senders across your accounts; opens the first account with any",
    );
  });

  test("one account's count needs no explanation, and none hides the entry", () => {
    expect(screenerEntry(desk("acct-1", 2, "acct-1"))?.hint).toBeUndefined();
    expect(screenerEntry(desk(null, 0))).toBeNull();
  });
});
