import { describe, expect, test } from "vitest";

import { byteOffsetToIndex, highlightSegments } from "./highlight";

describe("highlightSegments", () => {
  test("marks the understood run and keeps the rest", () => {
    expect(highlightSegments("fri 3 frday", [{ start: 0, end: 5 }])).toEqual([
      { start: 0, text: "fri 3", understood: true },
      { start: 5, text: " frday", understood: false },
    ]);
  });

  test("converts UTF-8 byte offsets to string indices", () => {
    const text = "é fri";
    expect(byteOffsetToIndex(text, 3)).toBe(2);
    expect(highlightSegments(text, [{ start: 3, end: 6 }])).toEqual([
      { start: 0, text: "é ", understood: false },
      { start: 2, text: "fri", understood: true },
    ]);
  });

});
