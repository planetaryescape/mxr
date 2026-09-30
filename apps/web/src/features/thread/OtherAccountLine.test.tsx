/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

const fetchAccounts = vi.hoisted(() => vi.fn<() => Promise<unknown>>());
vi.mock("@/features/accounts/api", () => ({ fetchAccounts }));

import { useUiPrefs } from "@/state/uiPrefsStore";

import { OtherAccountLine } from "./OtherAccountLine";

function renderLine(accountId: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <OtherAccountLine accountId={accountId} />
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
    expect(await screen.findByText(/In me@home\.example, not the account/)).toBeInTheDocument();
  });

  test("says nothing in the scoped account, or with all accounts shown", () => {
    useUiPrefs.getState().setAccountScope("work");
    const { unmount } = renderLine("work");
    expect(screen.queryByTestId("other-account-line")).toBeNull();
    unmount();
    useUiPrefs.getState().setAccountScope(null);
    renderLine("home");
    expect(screen.queryByTestId("other-account-line")).toBeNull();
    expect(fetchAccounts).not.toHaveBeenCalled();
  });
});
