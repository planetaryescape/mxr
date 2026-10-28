/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

const fetchAccounts = vi.hoisted(() => vi.fn<() => Promise<unknown>>());
vi.mock("@/features/accounts/api", () => ({ fetchAccounts }));

import { useUiPrefs } from "@/state/uiPrefsStore";

import { AccountIdentitiesProvider } from "@/features/accounts/useAccountIdentities";

import { OtherAccountLine } from "./OtherAccountLine";

function renderLine(accountId: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <AccountIdentitiesProvider>
        <OtherAccountLine accountId={accountId} />
      </AccountIdentitiesProvider>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  useUiPrefs.getState().setAccountScope(null);
  vi.clearAllMocks();
});

describe("OtherAccountLine", () => {
  test("names the conversation's own account when the app is scoped to another", async () => {
    fetchAccounts.mockResolvedValue({
      accounts: [
        { account_id: "work", name: "Work", email: "me@work.example" },
        { account_id: "home", name: "Home", email: "me@home.example" },
      ],
    });
    useUiPrefs.getState().setAccountScope("work");
    renderLine("home");
    expect(
      await screen.findByText(/In Home · me@home\.example · not the account/),
    ).toBeInTheDocument();
  });

  test("shows the account for one account too, and says nothing for an unknown account", async () => {
    fetchAccounts.mockResolvedValue({
      accounts: [{ account_id: "home", name: "Home", email: "me@home.example", enabled: true }],
    });
    renderLine("home");
    expect(await screen.findByTestId("other-account-line")).toHaveTextContent(
      "In Home · me@home.example",
    );
    cleanup();
    renderLine("work");
    await waitFor(() => expect(fetchAccounts).toHaveBeenCalled());
    expect(screen.queryByTestId("other-account-line")).toBeNull();
  });
});
