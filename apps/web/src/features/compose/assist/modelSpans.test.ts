import { describe, expect, test } from "vitest";

import { addedSpans, mapModelSpans, NO_SPANS } from "./modelSpans";

describe("mapModelSpans", () => {
  const before = "Hi Pat,\n\nFriday works.\n\n-- \nBK";
  const span = { start: 9, end: 20 }; // "Friday works"

  test("a span survives an edit that happens after it, and shifts past an edit before it", () => {
    const after = before.replace("-- \nBK", "-- \nBrian");
    expect(mapModelSpans([span], before, after)).toEqual([span]);

    const earlier = before.replace("Hi Pat", "Hello Pat");
    expect(mapModelSpans([span], before, earlier)).toEqual([{ start: 12, end: 23 }]);
  });

  test("typing inside a span clears its mark", () => {
    const after = before.replace("Friday", "Friday,");
    expect(mapModelSpans([span], before, after)).toEqual([]);
  });

  test("a span survives typing directly after it", () => {
    const after = before.replace("Friday works.", "Friday works.!");
    expect(mapModelSpans([span], before, after)).toEqual([span]);
  });

  test("an unchanged body keeps every span", () => {
    expect(mapModelSpans([span], before, before)).toEqual([span]);
  });
});

describe("addedSpans", () => {
  test("marks only the added runs, at the text's start in the body", () => {
    const pieces = [
      { kind: "kept" as const, text: "Thursday works. " },
      { kind: "removed" as const, text: "numbers" },
      { kind: "added" as const, text: "Numbers" },
      { kind: "gap" as const, text: "[[?: a time]]" },
    ];
    const spans = addedSpans(pieces, 100);
    expect(spans).toEqual([{ start: 100 + 16, end: 100 + 16 + 7 }]);
  });

  test("an unmarked draft stays on one shared empty list", () => {
    const before = "Hi\n\n-- \nBK";
    expect(mapModelSpans(NO_SPANS, before, "Hi there\n\n-- \nBK")).toBe(NO_SPANS);
  });
});
