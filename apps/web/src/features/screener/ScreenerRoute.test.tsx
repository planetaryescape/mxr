/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

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
}));

vi.mock("@/features/accounts/api", () => ({
  fetchAccounts: accounts.fetchAccounts,
}));

vi.mock("./api", () => ({
  fetchScreenerQueue: screener.fetchScreenerQueue,
  setScreenerDecision: screener.setScreenerDecision,
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn<(message: string) => void>(),
  },
}));

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
  });

  afterEach(() => {
    vi.clearAllMocks();
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
});
