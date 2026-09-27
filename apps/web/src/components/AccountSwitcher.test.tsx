/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { AccountSwitcher } from "./AccountSwitcher";
import { useUiPrefs } from "@/state/uiPrefsStore";

const accountsApi = vi.hoisted(() => ({
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
}));

vi.mock("@tanstack/react-router", () => ({
  Link: ({ to, children, ...rest }: { to: string; children: ReactNode }) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
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

function openMenu() {
  fireEvent.pointerDown(screen.getByRole("button", { name: /switch account/i }), {
    button: 0,
    ctrlKey: false,
  });
}

const work = {
  account_id: "account-1",
  key: "work",
  name: "Work",
  email: "work@example.com",
  provider_kind: "gmail",
  enabled: true,
  is_default: true,
};
const personal = {
  account_id: "account-2",
  key: "personal",
  name: "Personal",
  email: "me@example.com",
  provider_kind: "imap",
  enabled: true,
  is_default: false,
};

describe("AccountSwitcher", () => {
  beforeEach(() => {
    act(() => useUiPrefs.getState().setAccountScope(null));
    accountsApi.fetchAccounts.mockResolvedValue({ accounts: [work, personal] });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("with several accounts it shows all of them until one is picked", async () => {
    renderWithQueryClient(<AccountSwitcher />);

    expect(await screen.findByText("2 accounts")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Account: All accounts. Switch account" }),
    ).toBeVisible();

    openMenu();
    expect(await screen.findByRole("menuitem", { name: /Personal/ })).toBeVisible();
    fireEvent.click(screen.getByRole("menuitem", { name: /Personal/ }));

    expect(useUiPrefs.getState().accountScope).toBe("account-2");
    expect(
      await screen.findByRole("button", { name: "Account: Personal. Switch account" }),
    ).toBeVisible();
    expect(screen.getByText("me@example.com")).toBeVisible();
  });

  test("All accounts clears the scope", async () => {
    act(() => useUiPrefs.getState().setAccountScope("account-1"));
    renderWithQueryClient(<AccountSwitcher />);
    await screen.findByRole("button", { name: "Account: Work. Switch account" });

    openMenu();
    fireEvent.click(await screen.findByRole("menuitem", { name: /All accounts/ }));

    expect(useUiPrefs.getState().accountScope).toBeNull();
  });

  test("a single account is shown by name with no All accounts entry", async () => {
    accountsApi.fetchAccounts.mockResolvedValue({ accounts: [work] });
    renderWithQueryClient(<AccountSwitcher />);

    expect(
      await screen.findByRole("button", { name: "Account: Work. Switch account" }),
    ).toBeVisible();
    openMenu();
    await screen.findByRole("menuitem", { name: /Work/ });
    expect(screen.queryByRole("menuitem", { name: /All accounts/ })).not.toBeInTheDocument();
  });

  test("a scope pointing at a disabled account falls back to all mail", async () => {
    act(() => useUiPrefs.getState().setAccountScope("account-2"));
    accountsApi.fetchAccounts.mockResolvedValue({
      accounts: [work, { ...personal, enabled: false }],
    });
    renderWithQueryClient(<AccountSwitcher />);

    await vi.waitFor(() => expect(useUiPrefs.getState().accountScope).toBeNull());
    expect(
      await screen.findByRole("button", { name: "Account: Work. Switch account" }),
    ).toBeVisible();
  });
});
