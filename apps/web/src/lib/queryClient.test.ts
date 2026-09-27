import { describe, expect, test } from "vitest";

import { shouldRetryQuery } from "./queryClient";
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
