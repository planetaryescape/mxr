import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { mailboxActions, syncNow } from "./actions";
import { BridgeRequestError } from "@/api/client";
import type { ActionContext } from "@/lib/actions/types";
import { useConnectionStore } from "@/state/connectionStore";
import { useModals } from "@/state/modalStore";

const client = vi.hoisted(() => ({
  apiFetch: vi.fn<(path: string, init?: unknown) => Promise<unknown>>(),
}));
vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch: client.apiFetch,
}));

const toast = vi.hoisted(() => ({
  success: vi.fn<(message: string, options?: unknown) => void>(),
  error: vi.fn<(message: string, options?: { description?: string }) => void>(),
  info: vi.fn<(message: string) => void>(),
}));
vi.mock("sonner", () => ({ toast }));

const ctx: ActionContext = {
  path: "/m/inbox",
  activePane: "mailbox",
  scopes: ["global"],
  selectionCount: 0,
  accountCount: 1,
  hasFocusedThread: false,
  isFirstAccountOnly: true,
};

beforeEach(() => {
  useConnectionStore.setState({ syncProgress: undefined });
  useModals.setState({ commandPaletteOpen: false, rightRail: null });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("syncNow", () => {
  test("starts or joins every account without a redundant success toast", async () => {
    client.apiFetch.mockResolvedValue({
      results: [
        { account_name: "Work", result: "started", error: null },
        { account_name: "Personal", result: "joined" },
      ],
    });

    await syncNow();

    expect(client.apiFetch).toHaveBeenCalledWith("/api/v1/mail/sync/all", {
      method: "POST",
      body: {},
    });
    expect(toast.success).not.toHaveBeenCalled();
  });

  test("still triggers global sync while a different account is already running", async () => {
    useConnectionStore.setState({ syncProgress: { account_id: "a", current: 1, total: 9 } });
    client.apiFetch.mockResolvedValue({ results: [{ account_name: "Work", result: "joined" }] });

    await syncNow();

    expect(client.apiFetch).toHaveBeenCalled();
    expect(toast.info).not.toHaveBeenCalled();
  });

  test("shows failures next to the affected account", async () => {
    client.apiFetch.mockResolvedValue({
      results: [{ account_name: "Work", result: "failed", error: "offline" }],
    });

    await syncNow();

    expect(toast.error).toHaveBeenCalledWith("Some accounts couldn't sync", {
      description: "Work: offline",
    });
  });

  test("reports a failure to start", async () => {
    client.apiFetch.mockRejectedValue(new Error("daemon unreachable"));

    await syncNow();

    expect(toast.error).toHaveBeenCalledWith("Sync failed to start", {
      description: "daemon unreachable",
    });
  });

  test("explains when the daemon lacks the global sync endpoint", async () => {
    client.apiFetch.mockRejectedValue(new BridgeRequestError(404, "Not Found"));

    await syncNow();

    expect(toast.error).toHaveBeenCalledWith("Update mxr to sync all accounts", {
      description: "This daemon doesn't support global Sync now yet.",
    });
  });

  test("explains when there is nothing to sync", async () => {
    client.apiFetch.mockResolvedValue({ results: [] });

    await syncNow();

    expect(toast.info).toHaveBeenCalledWith("No enabled sync-capable accounts");
  });

  test("rejects an invalid per-account response", async () => {
    client.apiFetch.mockResolvedValue({
      results: [{ account_name: "Work", result: "sometimes" }],
    });

    await syncNow();

    expect(toast.error).toHaveBeenCalledWith("Sync failed to start", {
      description: "The daemon returned an invalid account sync result",
    });
  });
});

describe("mailbox palette actions", () => {
  test("Find an expert closes the palette and opens the expert finder", () => {
    useModals.setState({ commandPaletteOpen: true });
    const action = mailboxActions.find((item) => item.id === "mail.find-expert");

    void action?.run?.(ctx);

    expect(useModals.getState().commandPaletteOpen).toBe(false);
    expect(useModals.getState().rightRail).toEqual({ kind: "expert-finder", payload: undefined });
  });
});
