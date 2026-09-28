import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { claimLowTide, noteWork, useLowTide, type TideState } from "./lowTideMemory";

function blocked(): never {
  throw new Error("blocked");
}

/** A working Storage: some Node versions ship an inert global one. */
function memoryStorage(): Pick<Storage, "getItem" | "setItem" | "clear"> {
  const items = new Map<string, string>();
  return {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    clear: () => items.clear(),
  };
}

beforeEach(() => {
  vi.stubGlobal("localStorage", memoryStorage());
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("low tide memory", () => {
  it("is earned once per clearing, never by a place that was already clear", () => {
    expect(claimLowTide("desk")).toBe(false);
    noteWork("desk");
    expect(claimLowTide("desk")).toBe(true);
    expect(claimLowTide("desk")).toBe(false);
    noteWork("desk");
    expect(claimLowTide("desk")).toBe(true);
  });

  it("keeps places apart", () => {
    noteWork("reading");
    expect(claimLowTide("paper_trail")).toBe(false);
    expect(claimLowTide("reading")).toBe(true);
  });

  it("never shows when storage is unavailable, and never throws", () => {
    vi.stubGlobal("localStorage", { getItem: blocked, setItem: blocked });
    expect(() => noteWork("desk")).not.toThrow();
    expect(claimLowTide("desk")).toBe(false);
  });

  it("shows while the place stays clear and goes when work comes back", () => {
    const { result, rerender } = renderHook(
      ({ state }) => useLowTide("desk", state === "loading", state === "work"),
      { initialProps: { state: "loading" as TideState } },
    );
    expect(result.current).toBe(false);
    act(() => rerender({ state: "work" }));
    expect(result.current).toBe(false);
    act(() => rerender({ state: "clear" }));
    expect(result.current).toBe(true);
    act(() => rerender({ state: "work" }));
    expect(result.current).toBe(false);
  });

  it("a revisit to a clear desk is calm, not a moment", () => {
    noteWork("desk");
    claimLowTide("desk");
    const { result } = renderHook(() => useLowTide("desk", false, false));
    expect(result.current).toBe(false);
  });
});
