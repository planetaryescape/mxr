import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DEAD_END_NOTICE_MS, useKeyScope } from "./keyScopeStore";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("dead-end notice", () => {
  test("shows the sequence, then clears it", () => {
    useKeyScope.getState().showDeadEnd("g U");
    expect(useKeyScope.getState().deadEnd).toBe("g U");

    vi.advanceTimersByTime(DEAD_END_NOTICE_MS);
    expect(useKeyScope.getState().deadEnd).toBeNull();
  });

  test("a second dead end restarts the clock", () => {
    useKeyScope.getState().showDeadEnd("g U");
    vi.advanceTimersByTime(DEAD_END_NOTICE_MS - 100);
    useKeyScope.getState().showDeadEnd("g z");
    vi.advanceTimersByTime(200);

    expect(useKeyScope.getState().deadEnd).toBe("g z");
  });
});
