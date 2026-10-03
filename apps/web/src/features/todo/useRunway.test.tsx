import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { useUiPrefs } from "@/state/uiPrefsStore";

import { runwayFixture } from "./testing";

const fetchRunway = vi.fn<(account: string | null, markSeen: boolean) => Promise<unknown>>();
vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  fetchRunway: (account: string | null, markSeen: boolean) => fetchRunway(account, markSeen),
}));

const { useRunway } = await import("./TodoRoute");

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>;
}

afterEach(() => {
  fetchRunway.mockReset();
  act(() => useUiPrefs.setState({ accountScope: null }));
});

describe("useRunway", () => {
  test("each account's first look marks it seen and keeps its own expired count", async () => {
    fetchRunway.mockImplementation(async (account) =>
      runwayFixture({ expired_since_last_looked: account === "b" ? 2 : 5 }),
    );
    act(() => useUiPrefs.setState({ accountScope: "a" }));
    const { result } = renderHook(() => useRunway(), { wrapper });
    await waitFor(() => expect(result.current.expiredOnOpen).toBe(5));
    expect(fetchRunway).toHaveBeenLastCalledWith("a", true);

    act(() => useUiPrefs.setState({ accountScope: "b" }));
    await waitFor(() => expect(fetchRunway).toHaveBeenLastCalledWith("b", true));
    await waitFor(() => expect(result.current.expiredOnOpen).toBe(2));
  });
});
