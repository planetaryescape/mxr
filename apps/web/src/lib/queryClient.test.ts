import { onlineManager } from "@tanstack/react-query";
import { describe, expect, test } from "vitest";

import { createQueryClient, shouldRetryQuery } from "./queryClient";
import { BridgeRequestError, UnauthorizedError } from "@/api/client";

describe("query retries", () => {
  test("answers that won't change are not retried", () => {
    expect(shouldRetryQuery(0, new BridgeRequestError(404, "thread not found"))).toBe(false);
    expect(shouldRetryQuery(0, new BridgeRequestError(400, "invalid thread id"))).toBe(false);
    expect(shouldRetryQuery(0, new UnauthorizedError())).toBe(false);
  });

  test("transient failures still retry, twice", () => {
    for (const err of [
      new BridgeRequestError(502, "daemon unavailable"),
      new BridgeRequestError(429, "slow down"),
      new BridgeRequestError(408, "timeout"),
      new TypeError("fetch failed"),
    ]) {
      expect(shouldRetryQuery(0, err)).toBe(true);
      expect(shouldRetryQuery(2, err)).toBe(false);
    }
  });
});

describe("mutations while the daemon is down", () => {
  test("a scoped mutation runs (and fails) at once instead of waiting to replay", async () => {
    const client = createQueryClient();
    onlineManager.setOnline(false);
    try {
      let calls = 0;
      const mutation = client.getMutationCache().build(client, {
        scope: { id: "notification-chimes" },
        mutationFn: async () => {
          calls += 1;
          throw new Error("Couldn't reach mxr's daemon.");
        },
      });
      await expect(mutation.execute(undefined)).rejects.toThrow("Couldn't reach");
      expect(calls).toBe(1);
      expect(mutation.state.isPaused).toBe(false);
    } finally {
      onlineManager.setOnline(true);
    }
  });
});
