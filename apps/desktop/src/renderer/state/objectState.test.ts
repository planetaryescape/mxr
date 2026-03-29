import { describe, expect, it } from "vitest";
import { objectStateReducer } from "./objectState";

describe("objectStateReducer", () => {
  // --- field action ---

  it("returns the same object for no-op field updates", () => {
    const selected = new Set(["msg-1"]);
    const state = { selected };

    const next = objectStateReducer(state, {
      type: "field",
      key: "selected",
      updater: (current) => current,
    });

    expect(next).toBe(state);
  });

  it("returns a new object when a field value changes", () => {
    const state = { count: 5, label: "inbox" };

    const next = objectStateReducer(state, {
      type: "field",
      key: "count",
      updater: 10,
    });

    expect(next).not.toBe(state);
    expect(next.count).toBe(10);
    expect(next.label).toBe("inbox");
  });

  it("applies a function updater to the current field value", () => {
    const state = { count: 3, label: "inbox" };

    const next = objectStateReducer(state, {
      type: "field",
      key: "count",
      updater: (current) => (current as number) + 1,
    });

    expect(next.count).toBe(4);
    expect(next.label).toBe("inbox");
  });

  // --- patch action ---

  it("returns the same object for no-op patch updates", () => {
    const state = { ready: true, status: "ok" };

    const next = objectStateReducer(state, {
      type: "patch",
      patch: { ready: true },
    });

    expect(next).toBe(state);
  });

  it("merges changed keys from a patch", () => {
    const state = { ready: false, status: "loading" };

    const next = objectStateReducer(state, {
      type: "patch",
      patch: { ready: true, status: "ok" },
    });

    expect(next).not.toBe(state);
    expect(next).toEqual({ ready: true, status: "ok" });
  });

  it("preserves keys not included in the patch", () => {
    const state = { a: 1, b: 2, c: 3 };

    const next = objectStateReducer(state, {
      type: "patch",
      patch: { b: 99 },
    });

    expect(next).toEqual({ a: 1, b: 99, c: 3 });
  });

  // --- replace action ---

  it("replaces with a new state object", () => {
    const state = { x: 1 };
    const replacement = { x: 2 };

    const next = objectStateReducer(state, {
      type: "replace",
      next: replacement,
    });

    expect(next).toBe(replacement);
  });

  it("returns the same object when replace target is identical reference", () => {
    const state = { x: 1 };

    const next = objectStateReducer(state, {
      type: "replace",
      next: state,
    });

    expect(next).toBe(state);
  });
});
