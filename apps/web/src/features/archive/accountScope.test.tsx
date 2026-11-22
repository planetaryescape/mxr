import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { useUiPrefs } from "@/state/uiPrefsStore";

const apiFetch = vi.fn<(path: string) => Promise<unknown>>();
vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch: (path: string) => apiFetch(path),
}));

const { useLedger } = await import("./api");

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>;
}

function deferred<T>() {
  const handle: { resolve: (value: T) => void } = { resolve: () => undefined };
  const promise = new Promise<T>((done) => {
    handle.resolve = done;
  });
  return { promise, resolve: (value: T) => handle.resolve(value) };
}

afterEach(() => {
  apiFetch.mockReset();
  act(() => useUiPrefs.setState({ accountScope: null }));
});

describe("Archive's queries across an account switch", () => {
  test("the ledger never shows the previous account's records while the next loads", async () => {
    const pendingB = deferred<unknown>();
    apiFetch.mockImplementation((path) =>
      path.includes("account=b")
        ? pendingB.promise
        : Promise.resolve({ ledger: { total: 7, account: "a" } }),
    );
    act(() => useUiPrefs.setState({ accountScope: "a" }));
    const { result } = renderHook(() => useLedger({}), { wrapper });
    await waitFor(() => expect(result.current.data?.total).toBe(7));

    act(() => useUiPrefs.setState({ accountScope: "b" }));
    await waitFor(() =>
      expect(apiFetch).toHaveBeenLastCalledWith(expect.stringContaining("account=b")),
    );
    expect(result.current.data).toBeUndefined();

    pendingB.resolve({ ledger: { total: 2, account: "b" } });
    await waitFor(() => expect(result.current.data?.total).toBe(2));
  });
});
