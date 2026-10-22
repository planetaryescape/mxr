import { describe, expect, test } from "vitest";

import { optionalFlag } from "./searchParams";

describe("optionalFlag", () => {
  test("turns a flag on for every spelling the URL can produce", () => {
    expect(optionalFlag(true)).toBe(true);
    expect(optionalFlag("true")).toBe(true);
    expect(optionalFlag("1")).toBe(true);
    // `?unread=1` reaches the route as a number.
    expect(optionalFlag(1)).toBe(true);
  });

  test("leaves the flag off for anything else", () => {
    expect(optionalFlag(undefined)).toBeUndefined();
    expect(optionalFlag(false)).toBeUndefined();
    expect(optionalFlag("false")).toBeUndefined();
    expect(optionalFlag("0")).toBeUndefined();
    expect(optionalFlag(0)).toBeUndefined();
    expect(optionalFlag(["1"])).toBeUndefined();
  });
});
