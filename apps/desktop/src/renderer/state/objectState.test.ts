import { describe, expect, it } from "vitest";
import { objectStateReducer } from "./objectState";

describe("objectStateReducer", () => {
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

  it("returns the same object for no-op patch updates", () => {
    const state = { ready: true, status: "ok" };

    const next = objectStateReducer(state, {
      type: "patch",
      patch: { ready: true },
    });

    expect(next).toBe(state);
  });
});
