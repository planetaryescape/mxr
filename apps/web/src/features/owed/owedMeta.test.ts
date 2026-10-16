import { describe, expect, test } from "vitest";

import { owedMeta } from "./OwedRoute";

describe("owedMeta", () => {
  test("counts the people Now and Messages count, and says where the others are", () => {
    expect(owedMeta(4, { write_with: 3, others: 3 })).toBe(
      "3 people (4 conversations), 3 others in Messages",
    );
    expect(owedMeta(1, { write_with: 1, others: 0 })).toBe("1 person");
  });

  test("falls back to conversations from an older daemon", () => {
    expect(owedMeta(2, undefined)).toBe("2 conversations");
    expect(owedMeta(0, { write_with: 0, others: 5 })).toBeNull();
  });
});
