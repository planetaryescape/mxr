import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { PENDING_DELAY_MS, PENDING_MINIMUM_MS, useDelayedPending } from "./useDelayedPending";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function setup(pending: boolean) {
  return renderHook(({ loading }) => useDelayedPending(loading), {
    initialProps: { loading: pending },
  });
}

describe("useDelayedPending", () => {
  it("shows nothing for a load under 300 ms", () => {
    const { result, rerender } = setup(true);
    expect(result.current).toBe("quiet");
    act(() => vi.advanceTimersByTime(PENDING_DELAY_MS - 1));
    expect(result.current).toBe("quiet");
    rerender({ loading: false });
    expect(result.current).toBe("ready");
    act(() => vi.advanceTimersByTime(1000));
    expect(result.current).toBe("ready");
  });

  it("shows the skeleton after 300 ms and holds it at least 400 ms", () => {
    const { result, rerender } = setup(true);
    act(() => vi.advanceTimersByTime(PENDING_DELAY_MS));
    expect(result.current).toBe("skeleton");
    act(() => vi.advanceTimersByTime(50));
    rerender({ loading: false });
    expect(result.current).toBe("skeleton");
    act(() => vi.advanceTimersByTime(PENDING_MINIMUM_MS - 51));
    expect(result.current).toBe("skeleton");
    act(() => vi.advanceTimersByTime(1));
    expect(result.current).toBe("ready");
  });

  it("a skeleton shown longer than the minimum leaves as soon as data lands", () => {
    const { result, rerender } = setup(true);
    act(() => vi.advanceTimersByTime(PENDING_DELAY_MS + PENDING_MINIMUM_MS + 100));
    rerender({ loading: false });
    act(() => vi.advanceTimersByTime(0));
    expect(result.current).toBe("ready");
  });

  it("cached data never passes through a loading phase", () => {
    const { result } = setup(false);
    expect(result.current).toBe("ready");
  });
});
