import { describe, expect, test } from "vitest";

import {
  ago,
  effectiveHealth,
  healthTone,
  latestMail,
  syncLine,
  wentTo,
  worstAccount,
  type AccountFreshness,
  type Arrival,
  type Freshness,
} from "./copy";

const now = new Date("2026-10-07T09:42:00");
const minutesAgo = (minutes: number) => new Date(now.getTime() - minutes * 60_000).toISOString();
const minutesAhead = (minutes: number) => new Date(now.getTime() + minutes * 60_000).toISOString();

function account(overrides: Partial<AccountFreshness> = {}): AccountFreshness {
  return {
    account_id: "acct-1",
    account_name: "work",
    label: "Gmail",
    health: "ok",
    last_sync_ok_at: minutesAgo(1),
    sync_in_progress: false,
    ...overrides,
  };
}

describe("freshness words", () => {
  test("ages read in the largest whole unit and never in the future", () => {
    expect(ago(minutesAgo(0.5), now)).toBe("just now");
    expect(ago(minutesAgo(5), now)).toBe("5m ago");
    expect(ago(minutesAgo(125), now)).toBe("2h ago");
    expect(ago(minutesAgo(3 * 24 * 60), now)).toBe("3d ago");
    expect(ago(minutesAhead(2), now)).toBe("just now");
    expect(latestMail(null, now)).toBe("No mail yet");
    expect(latestMail(minutesAgo(5), now)).toBe("Latest mail 5m ago");
  });

  test("a good sync is calm until it ages past the limit, then says how old it is", () => {
    const ok = account({ last_sync_ok_at: minutesAgo(3) });
    expect(syncLine(ok, 900, now)).toBe("synced 3m ago");
    expect(healthTone(effectiveHealth(ok, 900, now))).toBe("ok");
    const later = new Date(now.getTime() + 2 * 60 * 60_000);
    expect(effectiveHealth(ok, 900, later)).toBe("stale");
    expect(syncLine(ok, 900, later)).toBe("Last sync 2h ago");
    expect(healthTone(effectiveHealth(ok, 900, later))).toBe("warn");
  });

  test("a rate limit names the provider and when it retries", () => {
    const paused = account({
      health: "paused",
      last_sync_error: {
        kind: "rate_limited",
        message: "Rate limited",
        retry_at: minutesAhead(8),
        consecutive_failures: 1,
      },
    });
    expect(syncLine(paused, 900, now)).toMatch(/^Gmail paused: rate limited, retrying 9:50/);
    const due = new Date(now.getTime() + 10 * 60_000);
    expect(syncLine(paused, 900, due)).toBe("Gmail paused: rate limited, retrying now");
  });

  test("auth and offline failures say what is wrong in plain words", () => {
    const auth = account({
      health: "failing",
      last_sync_error: { kind: "auth", message: "oauth", consecutive_failures: 2 },
    });
    expect(syncLine(auth, 900, now)).toBe("Gmail needs you to sign in again");
    expect(healthTone("failing")).toBe("bad");
    const offline = account({
      health: "failing",
      last_sync_ok_at: minutesAgo(12),
      last_sync_error: { kind: "offline", message: "dns", consecutive_failures: 1 },
    });
    expect(syncLine(offline, 900, now)).toBe("Gmail unreachable, last sync 12m ago");
    const retrying = {
      ...offline,
      last_sync_error: {
        kind: "offline" as const,
        message: "dns",
        consecutive_failures: 1,
        retry_at: minutesAhead(1),
      },
    };
    expect(syncLine(retrying, 900, now)).toMatch(/^Gmail unreachable, retrying 9:43/);
  });

  test("all accounts shows the worst sync state", () => {
    const failing = account({ account_id: "acct-2", health: "failing" });
    const data: Freshness = {
      generated_at: now.toISOString(),
      stale_after_secs: 900,
      accounts: [account(), failing],
      arrivals: [],
    };
    expect(worstAccount(data, now)?.account_id).toBe("acct-2");
  });

  test("an arrival says the mode it went to and why, in one word", () => {
    const arrival: Arrival = {
      account_id: "acct-1",
      message_id: "m-1",
      thread_id: "t-1",
      from: { email: "notifications@github.com", name: "GitHub" },
      subject: "Build passed",
      received_at: minutesAgo(5),
      in_inbox: true,
      modes: [
        {
          mode: "updates",
          name: "Updates",
          key: "g u",
          reason: "Here because: automated sender (rule).",
          also_in: "",
          tag: "automated",
        },
      ],
    };
    expect(wentTo(arrival)).toBe("→ Updates · automated");
    expect(wentTo({ ...arrival, modes: [] })).toBe("→ Inbox");
    expect(wentTo({ ...arrival, modes: [], in_inbox: false })).toBe("→ out of the inbox");
  });
});
