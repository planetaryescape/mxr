/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { getRegistry, snapshotActionContext } from "@/lib/actions";
import { installKeyDispatcher } from "@/lib/keys/dispatcher";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { ScreenerRoute } from "./ScreenerRoute";

const accounts = vi.hoisted(() => ({
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
}));

const screener = vi.hoisted(() => ({
  fetchScreenerQueue: vi.fn<(accountId: string) => Promise<unknown>>(),
  setScreenerDecision:
    vi.fn<
      (input: { accountId: string; senderEmail: string; disposition: string }) => Promise<unknown>
    >(),
  fetchScreenerDecisions: vi.fn<(accountId: string) => Promise<unknown>>(),
  clearScreenerDecision:
    vi.fn<(input: { accountId: string; senderEmail: string }) => Promise<unknown>>(),
}));

vi.mock("@/features/accounts/api", () => ({
  fetchAccounts: accounts.fetchAccounts,
}));

vi.mock("./api", () => ({
  fetchScreenerQueue: screener.fetchScreenerQueue,
  setScreenerDecision: screener.setScreenerDecision,
  fetchScreenerDecisions: screener.fetchScreenerDecisions,
  clearScreenerDecision: screener.clearScreenerDecision,
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn<(message: string) => void>(),
    error: vi.fn<(message: string, options?: unknown) => void>(),
  },
}));

let uninstall: () => void = () => {};

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

describe("ScreenerRoute", () => {
  beforeEach(() => {
    accounts.fetchAccounts.mockResolvedValue({
      accounts: [
        {
          account_id: "account-1",
          name: "Work",
          email: "me@example.com",
          provider_kind: "fake",
          enabled: true,
          is_default: true,
        },
      ],
    });
    screener.fetchScreenerQueue.mockResolvedValue({
      entries: [
        {
          sender_email: "unknown@example.com",
          display_name: "Unknown Sender",
          message_count: 3,
          latest_subject: "Question",
          latest_at: "2026-05-11T10:00:00Z",
        },
      ],
    });
    screener.setScreenerDecision.mockResolvedValue({ ok: true });
    screener.fetchScreenerDecisions.mockResolvedValue({
      decisions: [
        {
          account_id: "account-1",
          sender_email: "spammer@example.com",
          disposition: "deny",
          decided_at: "2026-05-10T08:00:00Z",
        },
      ],
    });
    screener.clearScreenerDecision.mockResolvedValue({ ok: true });
  });

  beforeEach(() => {
    // The app's one key dispatcher: screener keys reach the view through
    // the registry's "screener" scope, not a listener of its own.
    uninstall = installKeyDispatcher(window, {
      registry: getRegistry(),
      context: snapshotActionContext,
    });
  });

  afterEach(() => {
    uninstall();
    vi.clearAllMocks();
  });

  test("opens on the account the app is scoped to, as the desk's screener count does", async () => {
    accounts.fetchAccounts.mockResolvedValue({
      accounts: [
        { account_id: "account-1", name: "Work", email: "me@example.com", enabled: true },
        { account_id: "account-2", name: "Home", email: "me@home.example", enabled: true },
      ],
    });
    useUiPrefs.getState().setAccountScope("account-2");
    try {
      renderWithQueryClient(<ScreenerRoute />);
      await waitFor(() => expect(screener.fetchScreenerQueue).toHaveBeenCalledWith("account-2"));
      expect(screener.fetchScreenerQueue).not.toHaveBeenCalledWith("account-1");
    } finally {
      useUiPrefs.getState().setAccountScope(null);
    }
  });

  test("a linked account wins over the app's scope", async () => {
    accounts.fetchAccounts.mockResolvedValue({
      accounts: [
        { account_id: "account-1", name: "Work", email: "me@example.com", enabled: true },
        { account_id: "account-2", name: "Home", email: "me@home.example", enabled: true },
      ],
    });
    renderWithQueryClient(<ScreenerRoute account="account-2" />);
    await waitFor(() => expect(screener.fetchScreenerQueue).toHaveBeenCalledWith("account-2"));
    expect(screener.fetchScreenerQueue).not.toHaveBeenCalledWith("account-1");
  });

  test("pressing a allows the focused sender", async () => {
    renderWithQueryClient(<ScreenerRoute />);

    expect(await screen.findByText("Unknown Sender")).toBeVisible();

    fireEvent.keyDown(window, { key: "a" });

    await waitFor(() => {
      expect(screener.setScreenerDecision).toHaveBeenCalledWith({
        accountId: "account-1",
        senderEmail: "unknown@example.com",
        disposition: "allow",
      });
    });
  });

  test("j moves the cursor before deciding, and a failure is reported", async () => {
    screener.fetchScreenerQueue.mockResolvedValue({
      entries: [
        {
          sender_email: "first@example.com",
          display_name: "First",
          message_count: 1,
          latest_subject: "Hi",
          latest_at: "2026-05-11T10:00:00Z",
        },
        {
          sender_email: "second@example.com",
          display_name: "Second",
          message_count: 2,
          latest_subject: "Hello",
          latest_at: "2026-05-11T11:00:00Z",
        },
      ],
    });
    screener.setScreenerDecision.mockRejectedValue(new Error("daemon said no"));
    const { toast } = await import("sonner");
    renderWithQueryClient(<ScreenerRoute />);

    expect(await screen.findByText("Second")).toBeVisible();
    expect(screen.getByText("1 message", { exact: false })).toBeVisible();
    fireEvent.keyDown(window, { key: "j" });
    fireEvent.keyDown(window, { key: "d" });

    await waitFor(() => {
      expect(screener.setScreenerDecision).toHaveBeenCalledWith({
        accountId: "account-1",
        senderEmail: "second@example.com",
        disposition: "deny",
      });
    });
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
  });

  test("shows a skeleton, not an empty queue, while loading", async () => {
    screener.fetchScreenerQueue.mockReturnValue(new Promise(() => {}));
    renderWithQueryClient(<ScreenerRoute />);

    expect(await screen.findByRole("status", { name: /loading screener queue/i })).toBeVisible();
    expect(screen.queryByText("Queue empty")).not.toBeInTheDocument();
  });

  test("Decisions tab lists decisions and clears one", async () => {
    renderWithQueryClient(<ScreenerRoute />);

    const decisionsTab = await screen.findByRole("tab", { name: /decisions/i });
    fireEvent.click(decisionsTab);

    expect(await screen.findByText("spammer@example.com")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /^clear$/i }));

    await waitFor(() => {
      expect(screener.clearScreenerDecision).toHaveBeenCalledWith({
        accountId: "account-1",
        senderEmail: "spammer@example.com",
      });
    });
  });
});
