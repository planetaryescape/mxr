import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { mailboxActions, syncNow } from "./actions";
import { BridgeRequestError } from "@/api/client";
import type { ActionContext } from "@/lib/actions/types";
import { useConnectionStore } from "@/state/connectionStore";
import { useModals } from "@/state/modalStore";

const client = vi.hoisted(() => ({
  apiFetch: vi.fn<(path: string, init?: unknown) => Promise<unknown>>(),
}));
const bridgeHealth = vi.hoisted(() => ({
  fetch: vi.fn<() => Promise<{ protocol_version?: number }>>(),
}));
vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch: client.apiFetch,
}));
vi.mock("@/lib/bridgeHealth", () => ({ fetchBridgeHealth: bridgeHealth.fetch }));

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
  bridgeHealth.fetch.mockResolvedValue({ protocol_version: 6 });
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
    expect(bridgeHealth.fetch).toHaveBeenCalledOnce();
    expect(toast.success).not.toHaveBeenCalled();
  });

  test.each([{ protocol: 4 }, { protocol: 5 }, { protocol: undefined }])(
    "does not start sync when bridge protocol is $protocol",
    async ({ protocol }) => {
      bridgeHealth.fetch.mockResolvedValue(
        protocol === undefined ? {} : { protocol_version: protocol },
      );

      await syncNow();

      expect(client.apiFetch).not.toHaveBeenCalled();
      expect(toast.error).toHaveBeenCalledWith(
        "Update mxr to sync all accounts",
        expect.objectContaining({
          description: expect.stringContaining(
            "This version of mxr needs updating before it can sync all accounts.",
          ),
        }),
      );
      expect(toast.error).toHaveBeenCalledWith(
        "Update mxr to sync all accounts",
        expect.objectContaining({ description: expect.stringContaining("Update mxr:") }),
      );
    },
  );

  test("does not start sync when the compatibility check fails", async () => {
    bridgeHealth.fetch.mockRejectedValue(new Error("health unavailable"));

    await syncNow();

    expect(client.apiFetch).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith(
      "Couldn't verify mxr compatibility",
      expect.objectContaining({
        description: expect.stringContaining("mxr couldn't be reached, so sync wasn't started"),
      }),
    );
    expect(toast.error).toHaveBeenCalledWith(
      "Couldn't verify mxr compatibility",
      expect.objectContaining({
        description: expect.stringContaining("health unavailable"),
      }),
    );
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

  test.each([404, 405])(
    "explains when the bridge lacks the global sync endpoint (%i)",
    async (status) => {
      client.apiFetch.mockRejectedValue(new BridgeRequestError(status, "Unsupported route"));

      await syncNow();

      expect(toast.error).toHaveBeenCalledWith("Update mxr to sync all accounts", {
        description: expect.stringContaining("This daemon doesn't support global Sync now yet."),
      });
    },
  );

  test("explains a daemon protocol race after compatibility preflight", async () => {
    client.apiFetch.mockRejectedValue(
      new BridgeRequestError(
        426,
        "Global sync requires daemon IPC protocol 6; running daemon uses 5.",
        "protocol_mismatch",
      ),
    );

    await syncNow();

    expect(client.apiFetch).toHaveBeenCalledOnce();
    expect(toast.error).toHaveBeenCalledWith(
      "Update mxr to sync all accounts",
      expect.objectContaining({
        description: expect.stringContaining(
          "This version of mxr needs updating before it can sync all accounts.",
        ),
      }),
    );
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
