import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { mailboxActions, syncNow } from "./actions";
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
  test("starts a sync", async () => {
    client.apiFetch.mockResolvedValue({});

    await syncNow();

    expect(client.apiFetch).toHaveBeenCalledWith("/api/v1/mail/sync", { method: "POST", body: {} });
    expect(toast.success).toHaveBeenCalledWith("Sync started", expect.anything());
  });

  test("doesn't start a second sync while one is running", async () => {
    useConnectionStore.setState({ syncProgress: { account_id: "a", current: 1, total: 9 } });

    await syncNow();

    expect(client.apiFetch).not.toHaveBeenCalled();
    expect(toast.info).toHaveBeenCalledWith("A sync is already running");
  });

  test("reports a failure to start", async () => {
    client.apiFetch.mockRejectedValue(new Error("daemon unreachable"));

    await syncNow();

    expect(toast.error).toHaveBeenCalledWith("Sync failed to start", {
      description: "daemon unreachable",
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
