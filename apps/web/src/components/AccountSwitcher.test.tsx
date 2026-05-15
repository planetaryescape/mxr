/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { AccountSwitcher } from "./AccountSwitcher";

const accountsApi = vi.hoisted(() => ({
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
}));

vi.mock("@/features/accounts/api", () => ({
  fetchAccounts: accountsApi.fetchAccounts,
}));

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

describe("AccountSwitcher", () => {
  beforeEach(() => {
    accountsApi.fetchAccounts.mockResolvedValue({
      accounts: [
        {
          account_id: "account-1",
          key: "work",
          name: "Work",
          email: "work@example.com",
          provider_kind: "gmail",
          enabled: true,
          is_default: true,
        },
        {
          account_id: "account-2",
          key: "personal",
          name: "Personal",
          email: "me@example.com",
          provider_kind: "imap",
          enabled: true,
          is_default: false,
        },
      ],
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("shows the default account and lists configured accounts", async () => {
    renderWithQueryClient(<AccountSwitcher />);

    expect(await screen.findByText("Work")).toBeVisible();
    expect(screen.getByText("work@example.com")).toBeVisible();

    fireEvent.pointerDown(screen.getByRole("button", { name: /account switcher/i }), {
      button: 0,
      ctrlKey: false,
    });

    expect(await screen.findByText("default")).toBeVisible();
    expect(screen.getByText("Personal")).toBeVisible();
    expect(screen.getByText("me@example.com")).toBeVisible();
    expect(screen.queryByText(/no accounts loaded/i)).not.toBeInTheDocument();
  });
});
