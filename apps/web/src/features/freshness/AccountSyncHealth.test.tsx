/* @vitest-environment jsdom */

import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { useSyncHealthToasts } from "./AccountSyncHealth";
import type { AccountFreshness, Freshness } from "./copy";

const query = vi.hoisted(() => ({ data: undefined as Freshness | undefined }));
const toastMock = vi.hoisted(() => ({
  warning: vi.fn<(message: string, options?: unknown) => void>(),
  success: vi.fn<(message: string, options?: unknown) => void>(),
}));

vi.mock("./api", () => ({ useAllAccountsFreshness: () => ({ data: query.data }) }));
vi.mock("sonner", () => ({ toast: toastMock }));
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => vi.fn<() => Promise<void>>() }));

const start = new Date("2026-10-07T09:42:00");
const at = (minutes: number) => new Date(start.getTime() + minutes * 60_000).toISOString();

function account(overrides: Partial<AccountFreshness> = {}): AccountFreshness {
  return {
    account_id: "acct-1",
    account_name: "Work",
    label: "work@example.com",
    health: "ok",
    last_sync_ok_at: at(-2),
    sync_in_progress: false,
    ...overrides,
  };
}

function reply(...accounts: AccountFreshness[]): Freshness {
  return { generated_at: start.toISOString(), stale_after_secs: 900, accounts, arrivals: [] };
}

const offline = account({
  health: "failing",
  last_sync_error: {
    kind: "offline",
    message: "Provider error: connection refused",
    retry_at: at(5),
    consecutive_failures: 2,
  },
});

describe("account sync health", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(start);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
    query.data = undefined;
  });

  test("toasts once when an account starts failing and once when it recovers, never per retry", () => {
    query.data = reply(account());
    const { rerender } = renderHook(() => useSyncHealthToasts());
    expect(toastMock.warning).not.toHaveBeenCalled();

    query.data = reply(offline);
    rerender();
    expect(toastMock.warning).toHaveBeenCalledOnce();
    expect(toastMock.warning).toHaveBeenCalledWith(
      "work@example.com can't sync",
      expect.objectContaining({ id: "sync-health-acct-1" }),
    );

    query.data = reply({
      ...offline,
      last_sync_error: { ...offline.last_sync_error!, consecutive_failures: 3 },
    });
    rerender();
    expect(toastMock.warning).toHaveBeenCalledOnce();

    query.data = reply(account({ last_sync_ok_at: at(0) }));
    rerender();
    expect(toastMock.success).toHaveBeenCalledWith("work@example.com is syncing again", {
      id: "sync-health-acct-1",
    });
  });
});
