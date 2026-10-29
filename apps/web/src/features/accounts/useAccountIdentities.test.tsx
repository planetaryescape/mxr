/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { useUiPrefs } from "@/state/uiPrefsStore";

import { AccountIdentitiesProvider, useAccountIdentities } from "./useAccountIdentities";

const accountsApi = vi.hoisted(() => ({
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
}));
vi.mock("./api", () => ({ fetchAccounts: accountsApi.fetchAccounts }));

function renderIdentities(accounts: unknown[]) {
  accountsApi.fetchAccounts.mockResolvedValue({ accounts });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>
      <AccountIdentitiesProvider>{children}</AccountIdentitiesProvider>
    </QueryClientProvider>
  );
  return renderHook(() => useAccountIdentities(), { wrapper });
}

const work = {
  account_id: "work",
  name: "Work",
  email: "bk@work.example",
  enabled: true,
  identity: { color: "sky", tag: "W" },
};
const home = {
  account_id: "home",
  name: "Home",
  email: "bk@home.example",
  enabled: true,
  identity: { color: "amber", tag: "H" },
};

afterEach(() => {
  useUiPrefs.getState().setAccountScope(null);
  accountsApi.fetchAccounts.mockReset();
});

describe("useAccountIdentities", () => {
  test("marks show for several enabled accounts with no scope", async () => {
    const { result } = renderIdentities([work, home]);
    await waitFor(() => expect(result.current.marksShown).toBe(true));
    expect(result.current.several).toBe(true);
    expect(result.current.byId.get("home")).toMatchObject({ tag: "H", slot: "amber" });
  });

  test("a scoped view or one account shows no marks", async () => {
    useUiPrefs.getState().setAccountScope("work");
    const scoped = renderIdentities([work, home]);
    await waitFor(() => expect(scoped.result.current.byId.size).toBe(2));
    expect(scoped.result.current.marksShown).toBe(false);
    expect(scoped.result.current.several).toBe(true);
  });

  test("one enabled account shows no marks", async () => {
    const { result } = renderIdentities([work, { ...home, enabled: false }]);
    await waitFor(() => expect(result.current.byId.size).toBe(2));
    expect(result.current.several).toBe(false);
    expect(result.current.marksShown).toBe(false);
  });

  test("a colour slot from a newer daemon draws as slate, and the account keeps its tag", async () => {
    const { result } = renderIdentities([{ ...work, identity: { color: "teal", tag: "W" } }, home]);
    await waitFor(() => expect(result.current.marksShown).toBe(true));
    expect(result.current.byId.get("work")).toMatchObject({ name: "Work", tag: "W", slot: "slate" });
    expect(result.current.byId.get("home")).toMatchObject({ slot: "amber" });
  });

  test("an older daemon with no identities shows no marks", async () => {
    const { result } = renderIdentities([
      { ...work, identity: undefined },
      { ...home, identity: undefined },
    ]);
    await waitFor(() => expect(result.current.accountsById.size).toBe(2));
    expect(result.current.byId.size).toBe(0);
    expect(result.current.marksShown).toBe(false);
  });

  test("an unknown colour slot falls back to slate, and a missing tag to the name's initial", async () => {
    const { result } = renderIdentities([
      { ...work, identity: { color: "teal", tag: "" } },
      { ...home, identity: { color: "amber", tag: "H" } },
    ]);
    await waitFor(() => expect(result.current.byId.size).toBe(2));
    expect(result.current.byId.get("work")).toMatchObject({ slot: "slate", tag: "W" });
  });
});
