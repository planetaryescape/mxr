import { afterEach, describe, expect, test, vi } from "vitest";

import { gmailAccountConfig, outlookAccountConfig, startAuthSession } from "./api";

const api = vi.hoisted(() => ({
  fetch: vi.fn<(path: string, opts?: unknown) => Promise<unknown>>(),
}));

vi.mock("@/api/client", () => ({
  apiFetch: api.fetch,
}));

function startBody(): Record<string, unknown> {
  const call = api.fetch.mock.calls.at(-1);
  const opts = call?.[1] as { body?: Record<string, unknown> } | undefined;
  return opts?.body ?? {};
}

describe("startAuthSession flow selection", () => {
  afterEach(() => vi.clearAllMocks());

  test("Gmail accounts use the loopback flow (auto), not device-code", async () => {
    api.fetch.mockResolvedValue({ session: { session_id: "s1", state: "starting" } });

    await startAuthSession(gmailAccountConfig("user@gmail.com"));

    // Gmail's bundled client is a Desktop app type; the device-code
    // endpoint rejects it ("invalid_client"). It must use the loopback
    // (Installed) flow, which the daemon resolves from "auto".
    expect(startBody().flow).toBe("auto");
  });

  test("Outlook accounts keep the device-code flow", async () => {
    api.fetch.mockResolvedValue({ session: { session_id: "s2", state: "starting" } });

    await startAuthSession(outlookAccountConfig("user@outlook.com"));

    // provider-outlook implements only the device-code flow; switching it
    // to loopback would break Outlook onboarding.
    expect(startBody().flow).toBe("device");
  });
});
