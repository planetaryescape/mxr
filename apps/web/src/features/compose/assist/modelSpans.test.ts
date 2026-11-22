import { describe, expect, test } from "vitest";

import { mapModelSpans } from "./modelSpans";

describe("mapModelSpans", () => {
  const before = "Hi Pat,\n\nFriday works.\n\n-- \nBK";
  const span = { start: 9, end: 20 };

  test("a span survives an edit that happens after it, and shifts past an edit before it", () => {
    const after = before.replace("-- \nBK", "-- \nBrian");
    expect(mapModelSpans([span], before, after)).toEqual([span]);

    const earlier = before.replace("Hi Pat", "Hello Pat");
    expect(mapModelSpans([span], before, earlier)).toEqual([{ start: 12, end: 23 }]);
  });
});
