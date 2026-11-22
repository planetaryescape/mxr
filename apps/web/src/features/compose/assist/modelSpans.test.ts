import { describe, expect, test } from "vitest";

import { addedSpans, carryModelSpans, keptRuns, mapModelSpans } from "./modelSpans";

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
});

describe("addedSpans", () => {
  test("marks only the added runs, at the text's start in the body", () => {
    const pieces = [
      { kind: "kept" as const, text: "Thursday works. ", gap: null },
      { kind: "removed" as const, text: "numbers", gap: null },
      { kind: "added" as const, text: "Numbers", gap: null },
      { kind: "kept" as const, text: "[[?: a time]]", gap: 0 },
    ];
    const spans = addedSpans(pieces, 100);
    expect(spans).toEqual([{ start: 100 + 16, end: 100 + 16 + 7 }]);
  });

  test("a run past the inserted text is clipped to it", () => {
    // The proof ends in a newline, which the insert drops.
    const pieces = [{ kind: "added" as const, text: "Hi\n", gap: null }];
    expect(addedSpans(pieces, 10, "Hi".length)).toEqual([{ start: 10, end: 12 }]);
  });
});

describe("carryModelSpans", () => {
  test("a span over kept text moves with it and keeps its mark", () => {
    // Body "Friday works." with a signature below; the proof adds a greeting.
    const kept = keptRuns(
      [
        { kind: "added", text: "Hi Pat,\n\n" },
        { kind: "kept", text: "Friday works." },
      ],
      "Friday works.",
      "Hi Pat,\n\nFriday works.",
    );
    expect(kept).toEqual([{ oldStart: 0, length: 13, newStart: 9 }]);
    const carried = carryModelSpans(
      [{ start: 0, end: 13 }],
      {
        oldRegionStart: 0,
        oldRegionEnd: 13,
        newRegionStart: 0,
        insertedLength: 22,
        suffixShift: 0,
      },
      kept ?? [],
    );
    expect(carried).toEqual([{ start: 9, end: 22 }]);
  });

  test("a span keeps the unchanged part of an edit and drops the edited part", () => {
    const kept = keptRuns(
      [
        { kind: "kept", text: "Friday" },
        { kind: "removed", text: " works." },
        { kind: "added", text: " is set." },
      ],
      "Friday works.",
      "Friday is set.",
    );
    const carried = carryModelSpans(
      [{ start: 0, end: 13 }],
      {
        oldRegionStart: 0,
        oldRegionEnd: 13,
        newRegionStart: 0,
        insertedLength: 14,
        suffixShift: 0,
      },
      kept ?? [],
    );
    expect(carried).toEqual([{ start: 0, end: 6 }]);
  });

  test("changes that do not spell both texts give no runs, so the caller falls back", () => {
    expect(keptRuns([{ kind: "kept", text: "Hi" }], "Hello", "Hi")).toBeNull();
    expect(keptRuns([{ kind: "kept", text: "Hi" }], "Hi", "Hey")).toBeNull();
  });
});
