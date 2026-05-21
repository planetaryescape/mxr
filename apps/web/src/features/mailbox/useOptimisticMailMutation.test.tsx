/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { toast } from "sonner";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { useOptimisticMailMutation } from "./useOptimisticMailMutation";
import { useSelection } from "@/state/selectionStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
}));

const api = vi.hoisted(() => ({
  archiveMessages: vi.fn<(ids: string[]) => Promise<unknown>>(),
  trashMessages: vi.fn<(ids: string[]) => Promise<unknown>>(),
  spamMessages: vi.fn<(ids: string[]) => Promise<unknown>>(),
  starMessages: vi.fn<(ids: string[], on: boolean) => Promise<unknown>>(),
  markReadMessages: vi.fn<(ids: string[], read: boolean) => Promise<unknown>>(),
  modifyLabels: vi.fn<(ids: string[], add: string[], remove: string[]) => Promise<unknown>>(),
  moveMessagesToLabel: vi.fn<(ids: string[], label: string) => Promise<unknown>>(),
  readAndArchiveMessages: vi.fn<(ids: string[]) => Promise<unknown>>(),
  undoMutation: vi.fn<(id: string) => Promise<unknown>>(),
}));

vi.mock("./api", () => ({
  archiveMessages: api.archiveMessages,
  trashMessages: api.trashMessages,
  spamMessages: api.spamMessages,
  starMessages: api.starMessages,
  markReadMessages: api.markReadMessages,
  modifyLabels: api.modifyLabels,
  moveMessagesToLabel: api.moveMessagesToLabel,
  readAndArchiveMessages: api.readAndArchiveMessages,
  undoMutation: api.undoMutation,
  shellKey: ["shell"],
}));

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => router.navigate,
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn<(message: string, opts?: unknown) => void>(),
    error: vi.fn<(message: string, opts?: unknown) => void>(),
  },
}));

const mailboxKey = ["mailbox", { lens: "inbox" }] as const;

const baseMailbox = {
  mailbox: {
    lensLabel: "Inbox",
    view: "threads",
    groups: [
      {
        id: "today",
        title: "Today",
        rows: [
          {
            id: "m1",
            unread: true,
            labels: [{ id: "lbl-inbox", kind: "system", name: "Inbox" }],
          },
          {
            id: "m2",
            unread: true,
            labels: [{ id: "lbl-inbox", kind: "system", name: "Inbox" }],
          },
          {
            id: "m3",
            unread: true,
            labels: [{ id: "lbl-inbox", kind: "system", name: "Inbox" }],
          },
        ],
      },
    ],
  },
};

function mutationSuccess(count: number) {
  return {
    ok: true,
    result: {
      requested: count,
      succeeded: count,
      skipped: 0,
      failed: 0,
    },
  };
}

function wrapper(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

describe("useOptimisticMailMutation — label/move/read-and-archive", () => {
  let client: QueryClient;

  beforeEach(() => {
    window.sessionStorage.clear();
    client = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    client.setQueryData(mailboxKey, structuredClone(baseMailbox));
    useSelection.setState({ ids: new Set(), lastClickedId: null, scope: null });
  });

  afterEach(() => {
    vi.clearAllMocks();
    window.sessionStorage.clear();
  });

  test("move(ids, target=Receipts) optimistically removes rows from current view and calls moveMessagesToLabel", async () => {
    api.moveMessagesToLabel.mockResolvedValue(mutationSuccess(2));

    const { result } = renderHook(
      () => useOptimisticMailMutation("move", { payload: { label: "Receipts" } }),
      { wrapper: wrapper(client) },
    );

    await act(async () => {
      await result.current.mutateAsync(["m1", "m2"]);
    });

    expect(api.moveMessagesToLabel).toHaveBeenCalledWith(["m1", "m2"], "Receipts");
    const after = client.getQueryData(mailboxKey) as typeof baseMailbox;
    const remaining = after.mailbox.groups[0]?.rows.map((r) => r.id) ?? [];
    expect(remaining).toEqual(["m3"]);
  });

  test("move rolls back the cache when the API rejects", async () => {
    api.moveMessagesToLabel.mockRejectedValue(new Error("server boom"));

    const { result } = renderHook(
      () => useOptimisticMailMutation("move", { payload: { label: "Receipts" } }),
      { wrapper: wrapper(client) },
    );

    await act(async () => {
      try {
        await result.current.mutateAsync(["m1", "m2"]);
      } catch {
        /* expected */
      }
    });

    await waitFor(() => {
      const after = client.getQueryData(mailboxKey) as typeof baseMailbox;
      expect(after.mailbox.groups[0]?.rows.map((r) => r.id)).toEqual(["m1", "m2", "m3"]);
    });
  });

  test("label-add(ids, label=Receipts) keeps rows visible and calls modifyLabels with add only", async () => {
    api.modifyLabels.mockResolvedValue(mutationSuccess(1));

    const { result } = renderHook(
      () => useOptimisticMailMutation("label-add", { payload: { label: "Receipts" } }),
      { wrapper: wrapper(client) },
    );

    await act(async () => {
      await result.current.mutateAsync(["m1"]);
    });

    expect(api.modifyLabels).toHaveBeenCalledWith(["m1"], ["Receipts"], []);
    // label-add does not remove the row — m1 must still be in the view.
    const after = client.getQueryData(mailboxKey) as typeof baseMailbox;
    expect(after.mailbox.groups[0]?.rows.map((r) => r.id)).toEqual(["m1", "m2", "m3"]);
  });

  test("read-and-archive removes rows from the current view and calls readAndArchiveMessages", async () => {
    api.readAndArchiveMessages.mockResolvedValue(mutationSuccess(3));

    const { result } = renderHook(() => useOptimisticMailMutation("read-and-archive"), {
      wrapper: wrapper(client),
    });

    await act(async () => {
      await result.current.mutateAsync(["m1", "m2", "m3"]);
    });

    expect(api.readAndArchiveMessages).toHaveBeenCalledWith(["m1", "m2", "m3"]);
    const after = client.getQueryData(mailboxKey) as typeof baseMailbox;
    expect(after.mailbox.groups).toEqual([]);
  });

  test("move throws and does not call API when payload.label is missing", async () => {
    const { result } = renderHook(() => useOptimisticMailMutation("move"), {
      wrapper: wrapper(client),
    });

    await act(async () => {
      try {
        await result.current.mutateAsync(["m1"]);
      } catch {
        /* expected */
      }
    });
    expect(api.moveMessagesToLabel).not.toHaveBeenCalled();
  });

  test("read rolls back and reports provider skips as an error", async () => {
    api.markReadMessages.mockResolvedValue({
      ok: false,
      result: {
        requested: 1,
        succeeded: 0,
        skipped: 1,
        failed: 0,
        accounts: [
          {
            account_id: "account-1",
            account_name: "personal",
            succeeded: 0,
            skipped: 1,
            failed: 0,
            error: "account unavailable: No sync provider configured",
          },
        ],
      },
    });

    const { result } = renderHook(() => useOptimisticMailMutation("read"), {
      wrapper: wrapper(client),
    });

    let caught: unknown;
    await act(async () => {
      try {
        await result.current.mutateAsync(["m1"]);
      } catch (error) {
        caught = error;
      }
    });

    expect(caught).toBeInstanceOf(Error);
    expect((caught as Error).message).toContain("No messages changed");
    await waitFor(() => {
      const after = client.getQueryData(mailboxKey) as typeof baseMailbox;
      expect(after.mailbox.groups[0]?.rows[0]?.unread).toBe(true);
    });
    expect(toast.error).toHaveBeenCalledWith("Marked read failed", {
      description: expect.stringContaining("account unavailable"),
      action: expect.objectContaining({ label: "Re-auth personal" }),
    });
    expect(toast.success).not.toHaveBeenCalled();

    const [, toastOptions] = vi.mocked(toast.error).mock.calls[0] ?? [];
    const action = (toastOptions as { action?: { onClick: () => void } } | undefined)?.action;
    action?.onClick();

    expect(window.sessionStorage.getItem("mxr.account.reauth.request.v1")).toBe("account-1");
    expect(router.navigate).toHaveBeenCalledWith({
      to: "/accounts/$key",
      params: { key: "account-1" },
    });
  });
});
