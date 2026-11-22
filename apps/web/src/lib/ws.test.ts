import { describe, expect, test, vi } from "vitest";

import { parseFrame } from "./ws";

vi.mock("@/lib/tokenStorage", () => ({
  getBridgeWsUrl: () => "ws://127.0.0.1:7777",
  getToken: () => "test-token",
  onTokenChange: () => () => {},
}));

describe("parseFrame", () => {
  test("a SyncError frame reaches the handlers as a sync failure, not a lost bridge", () => {
    const frame = parseFrame(
      JSON.stringify({
        event: "SyncError",
        account_id: "a-1",
        error: "Provider error: rate limited",
      }),
    );
    expect(frame).toEqual({
      kind: "event",
      event: {
        event: "SyncError",
        type: "SyncError",
        account_id: "a-1",
        error: "Provider error: rate limited",
      },
    });
  });
});
