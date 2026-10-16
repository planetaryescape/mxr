import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { foldsFor, KEY_BURST_MS, resetLeaving } from "./leaving";

beforeEach(() => {
  resetLeaving();
  delete document.documentElement.dataset.motion;
});

afterEach(() => {
  delete document.documentElement.dataset.motion;
});

describe("foldsFor", () => {
  it("always folds for a pointer", () => {
    expect(foldsFor("pointer", 0)).toBe(true);
    expect(foldsFor("pointer", 10)).toBe(true);
  });

  it("folds the first key of a burst and drops the repeats", () => {
    expect(foldsFor("key", 1_000)).toBe(true);
    expect(foldsFor("key", 1_100)).toBe(false);
    expect(foldsFor("key", 1_200)).toBe(false);
  });

  it("measures the burst from the last key, so a held run never folds again", () => {
    expect(foldsFor("key", 1_000)).toBe(true);
    expect(foldsFor("key", 1_000 + KEY_BURST_MS - 1)).toBe(false);
    expect(foldsFor("key", 1_000 + 2 * KEY_BURST_MS - 2)).toBe(false);
  });

  it("folds again after a pause", () => {
    expect(foldsFor("key", 1_000)).toBe(true);
    expect(foldsFor("key", 1_000 + KEY_BURST_MS)).toBe(true);
  });

  it("never folds under reduced motion", () => {
    document.documentElement.dataset.motion = "reduced";
    expect(foldsFor("pointer", 0)).toBe(false);
    expect(foldsFor("key", 10_000)).toBe(false);
  });
});
