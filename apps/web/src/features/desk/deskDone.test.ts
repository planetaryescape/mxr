import { beforeEach, describe, expect, test, vi } from "vitest";

import { conversationKey } from "@/features/mailbox/rowKey";

import type { DeskRow } from "./api";
import { deskDoneItem, markDeskDone, useDeskDone } from "./deskDone";

const apiFetch = vi.hoisted(() =>
  vi.fn<(path: string, opts?: { body?: { items?: unknown[] } }) => Promise<unknown>>(),
);
vi.mock("@/api/client", () => ({ apiFetch }));
vi.mock("@/features/mail-actions/mailUndo", () => ({
  claimUndo: () => ({ settle: vi.fn<() => void>(), run: vi.fn<() => void>() }),
  offerUndo: vi.fn<() => void>(),
  performUndo: vi.fn<() => void>(),
}));
vi.mock("@/features/mail-actions/mailQueryInvalidation", () => ({
  invalidateMailQueries: () => Promise.resolve(),
}));
vi.mock("@/features/sound/player", () => ({ playSound: vi.fn<() => void>() }));
vi.mock("@/lib/daemonAvailability", () => ({
  refuseWhileDaemonDown: () => false,
}));
vi.mock("sonner", () => ({
  toast: { error: vi.fn<() => void>(), warning: vi.fn<() => void>() },
}));

function owedRow(account: string): DeskRow {
  return {
    lane: "owed",
    account_id: account,
    thread_id: "shared",
    message_id: `m-${account}`,
    message_ids: [`m-${account}`],
    counterparty_email: "x@example.com",
    subject: "Shared id",
    reason: "wrote to you",
    since: "2026-09-26T08:00:00Z",
    age_seconds: 1,
    usual_samples: 0,
    overdue: false,
    unread: false,
  };
}

beforeEach(() => apiFetch.mockReset());

describe("desk Done for a thread id held by two accounts", () => {
  test("Done on A's row sends A's account, not B's", async () => {
    apiFetch.mockResolvedValue({
      kind: "DeskItemsResolved",
      items: [],
      mutation_id: null,
    });
    await markDeskDone([deskDoneItem(owedRow("A"))]);
    expect(apiFetch.mock.calls[0]?.[1]?.body?.items).toEqual([
      { thread_id: "shared", account_id: "A", lane: "owed" },
    ]);
  });

  test("both accounts' rows in one Done stay two items", async () => {
    apiFetch.mockResolvedValue({
      kind: "DeskItemsResolved",
      items: [],
      mutation_id: null,
    });
    await markDeskDone([deskDoneItem(owedRow("A")), deskDoneItem(owedRow("B"))]);
    expect(apiFetch.mock.calls[0]?.[1]?.body?.items).toHaveLength(2);
  });

  test("only A's row is hidden while its Done is in flight", async () => {
    let hidden: ReadonlySet<string> = new Set();
    apiFetch.mockImplementation(() => {
      hidden = useDeskDone.getState().hidden;
      return Promise.resolve({
        kind: "DeskItemsResolved",
        items: [],
        mutation_id: null,
      });
    });
    await markDeskDone([deskDoneItem(owedRow("A"))]);
    expect([...hidden]).toEqual([conversationKey("shared", "A")]);
    expect(useDeskDone.getState().hidden.size).toBe(0);
  });
});
