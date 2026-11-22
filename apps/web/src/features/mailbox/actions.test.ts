import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { syncNow } from "./actions";
import { useConnectionStore } from "@/state/connectionStore";
import { useModals } from "@/state/modalStore";
import { EXPECTED_IPC_PROTOCOL_VERSION } from "@/lib/protocolCompatibility";

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

beforeEach(() => {
  useConnectionStore.setState({ syncProgress: undefined });
  useModals.setState({ commandPaletteOpen: false, rightRail: null });
  bridgeHealth.fetch.mockResolvedValue({ protocol_version: EXPECTED_IPC_PROTOCOL_VERSION });
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
});
