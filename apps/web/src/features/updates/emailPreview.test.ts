import { describe, expect, test } from "vitest";

import { previewLines } from "./emailPreview";

describe("previewLines", () => {
  test("markup stays text, and a missing or odd-typed body gives nothing", () => {
    expect(previewLines("<script>alert(1)</script> and more text")).toEqual([
      "<script>alert(1)</script> and more text",
    ]);
    for (const value of [undefined, null, "", "   \n\n", 5, {}, ["a b"]]) {
      expect(previewLines(value as never)).toEqual([]);
    }
  });
});
