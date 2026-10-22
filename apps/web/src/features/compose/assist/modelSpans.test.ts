import { describe, expect, test } from "vitest";

import { mapModelSpans } from "./modelSpans";

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
