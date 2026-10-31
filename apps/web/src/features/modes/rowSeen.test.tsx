import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, expect, test, vi } from "vitest";

import { useMarkRowSeen } from "./rowSeen";

const fetch = vi.fn<(...args: unknown[]) => Promise<unknown>>();
vi.mock("@/api/client", () => ({ apiFetch: (...args: unknown[]) => fetch(...args) }));
vi.mock("sonner", () => ({ toast: { error: vi.fn<(message: string) => void>() } }));
beforeEach(() => {
  fetch.mockReset();
  fetch.mockResolvedValue({ kind: "Ack" });
});

test.each(["todo", "updates"] as const)(
  "%s acknowledges only the opened revision and refetches durable state",
  async (mode) => {
    const client = new QueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useMarkRowSeen(mode), { wrapper });
    expect(fetch).not.toHaveBeenCalled(); // Mounting/selecting a list is not seeing a row.
    act(() =>
      result.current({
        id: "row",
        account_id: "account",
        unread: true,
        seen_revision: "old-revision",
      }),
    );
    await waitFor(() =>
      expect(invalidate).toHaveBeenCalledWith({
        queryKey: [mode === "todo" ? "todos" : "updates"],
      }),
    );
    expect(fetch).toHaveBeenCalledWith("/api/v1/mail/rows/seen", {
      method: "POST",
      body: { mode, account_id: "account", row_id: "row", revision: "old-revision" },
    });
  },
);

test("seen rows and payloads from older daemons never write", () => {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
  );
  const { result } = renderHook(() => useMarkRowSeen("todo"), { wrapper });
  act(() => {
    result.current({ id: "row", account_id: "account", unread: false, seen_revision: "rev" });
    result.current({ id: "row", account_id: "account", unread: true });
  });
  expect(fetch).not.toHaveBeenCalled();
});
