import { describe, expect, test } from "vitest";

import { nextAfterRemoval } from "./listAdvance";

const all = () => true;

describe("nextAfterRemoval", () => {
  test("opens the next row, the previous one at the end, nothing when alone", () => {
    expect(nextAfterRemoval(["a", "b", "c"], "b", all)).toBe("c");
    expect(nextAfterRemoval(["a", "b", "c"], "c", all)).toBe("b");
    expect(nextAfterRemoval(["a"], "a", all)).toBeNull();
  });

  test("skips rows that left with it (a batch, a sync)", () => {
    const left = new Set(["c", "d"]);
    expect(nextAfterRemoval(["a", "b", "c", "d", "e"], "c", (id) => !left.has(id))).toBe("e");
    expect(nextAfterRemoval(["a", "b", "c", "d"], "c", (id) => !left.has(id))).toBe("b");
  });

  test("never opens the removed row itself, even if it is still there", () => {
    expect(nextAfterRemoval(["a", "b"], "b", all)).toBe("a");
  });

  test("a row that was never in the list moves nothing", () => {
    expect(nextAfterRemoval(["a", "b"], "z", all)).toBeNull();
  });
});
