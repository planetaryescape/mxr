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
const recoveryState = vi.hoisted(() => ({ restored: false }));

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

vi.mock("@/features/recovery/RecoveryNotice", () => ({
  isRestoredProfile: (
    recovery: { restore_pending: boolean; source_owner_stopped: boolean } | null,
  ) => Boolean(recovery && (recovery.restore_pending || recovery.source_owner_stopped)),
  useRecoveryStatus: () => ({
    data: recoveryState.restored ? { restore_pending: false, source_owner_stopped: true } : null,
    isSuccess: true,
    isError: false,
  }),
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
    recoveryState.restored = false;
    act(() => useUiPrefs.getState().setAccountScope(null));
    accountsApi.fetchAccounts.mockResolvedValue({ accounts: [work, personal] });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("All accounts clears the scope", async () => {
    act(() => useUiPrefs.getState().setAccountScope("account-1"));
    renderWithQueryClient(<AccountSwitcher />);
    await screen.findByRole("button", { name: "Account: Work. Switch account" });

    openMenu();
    fireEvent.click(await screen.findByRole("menuitem", { name: /All accounts/ }));

    expect(useUiPrefs.getState().accountScope).toBeNull();
  });
});
