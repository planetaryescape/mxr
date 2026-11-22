import { describe, expect, it } from "vitest";

import { countLink, lineParts, linkedCounts, shownLine } from "./arrivalLine";
import { arrivals } from "./testing";

describe("the arrivals line", () => {
  it("links every count in the daemon's sentence and keeps the words around them", () => {
    const line = arrivals();
    const parts = lineParts(line.line, linkedCounts(line));
    expect(
      parts.map((part) => ("text" in part ? part.text : `[${part.count.label}]`)).join(""),
    ).toBe(
      "Since 08:12: 50 arrived. [8 Messages] · [10 Updates] · [31 Reading] · [1 spam]. Also [2 in To do].",
    );
  });

  it("leaves a label the sentence lacks unlinked instead of breaking it", () => {
    const parts = lineParts("Nothing new since 08:12.", [
      { bucket: "reading", count: 1, label: "1 Reading" },
    ]);
    expect(parts).toEqual([{ at: 0, text: "Nothing new since 08:12." }]);
  });

  it("swaps in the clear line only when nothing on Now waits", () => {
    expect(shownLine(arrivals(), false)).toBe(
      "Clear. All 50 emails since 08:12 are accounted for.",
    );
    expect(shownLine(arrivals(), true)).toMatch(/^Since 08:12: 50 arrived/);
    // Still sorting: the daemon sends no clear line, so the counts stay.
    expect(shownLine(arrivals({ clear_line: null }), false)).toMatch(/^Since/);
  });

  it("opens a count in the line's own window, so the list holds exactly that many", () => {
    const line = arrivals();
    expect(countLink(line, line.counts[2]!)).toEqual({
      bucket: "reading",
      since: "2026-10-07T08:12:00Z",
      until: "2026-10-07T12:00:01Z",
    });
  });

});
