/* @vitest-environment jsdom */

import { fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { AccountSyncDetails, SyncHealthWords, useSyncHealthToasts } from "./AccountSyncHealth";
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
    label: "Gmail",
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

  test("a row says the health in words, never colour alone", () => {
    query.data = reply(offline);
    render(<SyncHealthWords accountId="acct-1" />);
    const words = screen.getByTestId("account-sync-health");
    expect(words).toHaveTextContent(/^Can't sync, retrying 9:47/);
    expect(words).toHaveAttribute("data-tone", "bad");
  });

  test("the Sync section gives the next try, a plain reason and the raw error behind Details", () => {
    query.data = reply(offline);
    render(<AccountSyncDetails accountId="acct-1" />);
    const details = screen.getByTestId("account-sync-details");
    expect(details).toHaveTextContent("Last good sync2m ago");
    expect(details).toHaveTextContent(/Next try9:47/);
    expect(details).toHaveTextContent(/Can't reach Gmail\. Trying again at 9:47/);
    expect(screen.getByText("Details")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /sign in again/i })).toBeNull();
  });

  test("a signed-out account offers Sign in again", () => {
    query.data = reply(
      account({
        health: "failing",
        last_sync_error: { kind: "auth", message: "oauth revoked", consecutive_failures: 1 },
      }),
    );
    const onSignIn = vi.fn<() => void>();
    render(<AccountSyncDetails accountId="acct-1" onSignIn={onSignIn} />);
    expect(screen.getByText("Gmail signed mxr out. Sign in again to keep syncing.")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /sign in again/i }));
    expect(onSignIn).toHaveBeenCalledOnce();
  });

  test("toasts once when an account starts failing and once when it recovers, never per retry", () => {
    query.data = reply(account());
    const { rerender } = renderHook(() => useSyncHealthToasts());
    expect(toastMock.warning).not.toHaveBeenCalled();

    query.data = reply(offline);
    rerender();
    expect(toastMock.warning).toHaveBeenCalledOnce();
    expect(toastMock.warning).toHaveBeenCalledWith(
      "Work can't sync",
      expect.objectContaining({ id: "sync-health-acct-1" }),
    );

    // Another failed retry: still failing, nothing new to say.
    query.data = reply({
      ...offline,
      last_sync_error: { ...offline.last_sync_error!, consecutive_failures: 3 },
    });
    rerender();
    expect(toastMock.warning).toHaveBeenCalledOnce();

    query.data = reply(account({ last_sync_ok_at: at(0) }));
    rerender();
    expect(toastMock.success).toHaveBeenCalledWith("Work is syncing again", {
      id: "sync-health-acct-1",
    });
  });

  test("a retry that is still running is not a recovery", () => {
    query.data = reply(offline);
    const { rerender } = renderHook(() => useSyncHealthToasts());
    query.data = reply({ ...offline, sync_in_progress: true });
    rerender();
    expect(toastMock.success).not.toHaveBeenCalled();
  });

  test("an account already failing when the app opens does not toast", () => {
    query.data = reply(offline);
    renderHook(() => useSyncHealthToasts());
    expect(toastMock.warning).not.toHaveBeenCalled();
  });
});
