import { beforeEach, describe, expect, it, vi } from "vitest";

import { conversationKey } from "@/features/mailbox/rowKey";
import { conversationNotNow } from "@/features/mail-actions/notNow";
import { fakeChoice } from "@/features/time/testing";

import { deferredMessage, deferThreadsAt, deskLaneOf } from "./deskLater";
import { useDeskDone } from "./deskDone";

const apiFetch = vi.hoisted(() =>
  vi.fn<(path: string, opts?: { body?: Record<string, unknown> }) => Promise<unknown>>(),
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
vi.mock("sonner", () => ({ toast: { error: vi.fn<() => void>(), warning: vi.fn<() => void>() } }));

describe("deferredMessage", () => {
  const choice = fakeChoice(new Date(2026, 9, 6, 9, 0), {
    date_label: "Tuesday 6 October",
    time_label: "09:00",
  });

  it("names the exact time reply later comes back", () => {
    expect(deferredMessage(["reply_later"], choice)).toBe(
      "Reply later: back Tuesday 6 October, 09:00",
    );
  });

  it("says a wait comes back only if nobody replies", () => {
    expect(deferredMessage(["waiting"], choice)).toBe(
      "Back Tuesday 6 October, 09:00 if nobody replies",
    );
  });
});

describe("Not now for a thread id held by two accounts", () => {
  beforeEach(() => apiFetch.mockReset());

  it("sends the row's account with the time and hides only that account's row meanwhile", async () => {
    let hidden: ReadonlySet<string> = new Set();
    apiFetch.mockImplementation(() => {
      hidden = useDeskDone.getState().hidden;
      return Promise.resolve({ kind: "ThreadsDeferred", items: [], mutation_id: null });
    });
    const picker = conversationNotNow({
      threadId: "shared",
      accountId: "A",
      subject: "Shared id",
      waiting: false,
    });
    await picker.at("2026-10-30T09:00:00Z", "tomorrow");
    expect(apiFetch.mock.calls[0]?.[1]?.body).toMatchObject({
      thread_ids: ["shared"],
      thread_accounts: [{ thread_id: "shared", account_id: "A" }],
    });
    expect([...hidden]).toEqual([conversationKey("shared", "A")]);
  });

  it("keeps the bare request for data with no account", async () => {
    apiFetch.mockResolvedValue({ kind: "ThreadsDeferred", items: [], mutation_id: null });
    await deferThreadsAt(["old"], "2026-10-30T09:00:00Z", "tomorrow");
    expect(apiFetch.mock.calls[0]?.[1]?.body).not.toHaveProperty("thread_accounts");
  });

  it("knows no lane for a thread when nothing is loaded", () => {
    expect(deskLaneOf("shared", "A")).toBeUndefined();
  });
});
