import { describe, expect, test } from "vitest";

import { healthTone, syncLine, type AccountFreshness } from "./copy";

const now = new Date("2026-10-07T09:42:00");
const minutesAgo = (minutes: number) => new Date(now.getTime() - minutes * 60_000).toISOString();
const minutesAhead = (minutes: number) => new Date(now.getTime() + minutes * 60_000).toISOString();

function account(overrides: Partial<AccountFreshness> = {}): AccountFreshness {
  return {
    account_id: "acct-1",
    account_name: "work",
    label: "work@example.com",
    health: "ok",
    last_sync_ok_at: minutesAgo(1),
    sync_in_progress: false,
    ...overrides,
  };
}

describe("freshness words", () => {
  test("auth and offline failures say what is wrong in plain words", () => {
    const auth = account({
      health: "failing",
      last_sync_error: { kind: "auth", message: "oauth", consecutive_failures: 2 },
    });
    expect(syncLine(auth, 900, now)).toBe("work@example.com needs you to sign in again");
    expect(healthTone("failing")).toBe("bad");
    const offline = account({
      health: "failing",
      last_sync_ok_at: minutesAgo(12),
      last_sync_error: { kind: "offline", message: "dns", consecutive_failures: 1 },
    });
    expect(syncLine(offline, 900, now)).toBe("Can't reach work@example.com, last sync 12m ago");
    const retrying = {
      ...offline,
      last_sync_error: {
        kind: "offline" as const,
        message: "dns",
        consecutive_failures: 1,
        retry_at: minutesAhead(1),
      },
    };
    expect(syncLine(retrying, 900, now)).toMatch(/^Can't reach work@example.com, retrying 09:43/);
  });
});
