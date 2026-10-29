import { beforeEach, describe, expect, test, vi } from "vitest";

import { fetchMembership, fetchMemberships, modesByThread, type ThreadModes } from "./membership";

const apiFetch = vi.hoisted(() =>
  vi.fn<(path: string, opts?: { body?: Record<string, unknown> }) => Promise<unknown>>(),
);
vi.mock("@/api/client", () => ({ apiFetch }));

const modes = (account: string): ThreadModes =>
  ({
    thread_id: "shared",
    account_id: account,
    modes: [],
    in_inbox: true,
  }) as unknown as ThreadModes;

beforeEach(() => apiFetch.mockReset());

describe("mode membership for a thread id held by two accounts", () => {
  test("a row asks about its own account", async () => {
    apiFetch.mockResolvedValue({
      kind: "ModeMembership",
      threads: [modes("B")],
    });
    await fetchMembership("shared", "B");
    expect(apiFetch.mock.calls[0]?.[0]).toBe(
      "/api/v1/mail/modes/membership?thread_id=shared&account_id=B",
    );
  });

  test("a list asks with one pair per account and keeps both answers apart", async () => {
    apiFetch.mockResolvedValue({
      kind: "ModeMembership",
      threads: [modes("A"), modes("B")],
    });
    const answered = await fetchMemberships([
      { thread_id: "shared", account_id: "A" },
      { thread_id: "shared", account_id: "B" },
    ]);
    expect(apiFetch.mock.calls[0]?.[1]?.body).toEqual({
      thread_ids: ["shared"],
      thread_accounts: [
        { thread_id: "shared", account_id: "A" },
        { thread_id: "shared", account_id: "B" },
      ],
    });
    const map = modesByThread(answered);
    expect(map.get("thread-A:shared")?.account_id).toBe("A");
    expect(map.get("thread-B:shared")?.account_id).toBe("B");
    // A caller with no account reads the first entry by the bare id.
    expect(map.get("shared")?.account_id).toBe("A");
  });

  test("a bare list keeps the request it always sent", async () => {
    apiFetch.mockResolvedValue({ kind: "ModeMembership", threads: [] });
    await fetchMemberships([{ thread_id: "old" }]);
    expect(apiFetch.mock.calls[0]?.[1]?.body).toEqual({ thread_ids: ["old"] });
  });
});
