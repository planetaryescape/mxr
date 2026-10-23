import { describe, expect, test } from "vitest";

import { proofRuns, runsText } from "./proof";

const bytes = (text: string) => new TextEncoder().encode(text).length;

describe("proofRuns", () => {
  test("kept, added and removed runs come out in order", () => {
    const changes = [
      { kind: "kept" as const, text: "Thursday works. " },
      { kind: "removed" as const, text: "numbers" },
      { kind: "added" as const, text: "Numbers" },
      { kind: "kept" as const, text: " by " },
      { kind: "added" as const, text: "[[?: a time]]" },
    ];
    const proposalText = "Thursday works. Numbers by [[?: a time]]";
    const start = bytes("Thursday works. Numbers by ");
    const gaps = [{ label: "a time", start, end: start + bytes("[[?: a time]]") }];
    const runs = proofRuns(changes, gaps);
    expect(runs.map((run) => [run.kind, run.gap])).toEqual([
      ["kept", null],
      ["removed", null],
      ["added", null],
      ["kept", null],
      ["added", 0],
    ]);
    expect(runsText(runs)).toBe(proposalText);
  });

  test("a gap split by the word diff stays one gap across its runs", () => {
    // "[[?: the date]]" became "[[?: the exact date]]": kept, added, kept.
    const proposal = "Meet at [[?: the exact date]] tomorrow";
    const start = bytes("Meet at ");
    const gaps = [{ label: "the exact date", start, end: start + bytes("[[?: the exact date]]") }];
    const runs = proofRuns(
      [
        { kind: "kept", text: "Meet at [[?: the " },
        { kind: "added", text: "exact " },
        { kind: "kept", text: "date]] tomorrow" },
      ],
      gaps,
    );
    expect(runs.filter((run) => run.gap === 0)).toHaveLength(3);
    expect(runs.filter((run) => run.gap !== null).map((run) => run.gap)).toEqual([0, 0, 0]);
    expect(runsText(runs)).toBe(proposal);
  });

  test("a gap after non-ASCII text is found by its UTF-8 byte range", () => {
    // "é" and "—" are multi-byte: the gap's byte offsets are not character offsets.
    const proposal = "Café — [[?: when]] ok";
    const start = bytes("Café — ");
    const gaps = [{ label: "when", start, end: start + bytes("[[?: when]]") }];
    const runs = proofRuns([{ kind: "kept", text: proposal }], gaps);
    expect(runs.map((run) => [run.text, run.gap])).toEqual([
      ["Café — ", null],
      ["[[?: when]]", 0],
      [" ok", null],
    ]);
  });

  test("a removed run inside a gap belongs to it, so it does not split the gap", () => {
    const gaps = [{ label: "x", start: 0, end: bytes("[[?: the date]]") }];
    const runs = proofRuns(
      [
        { kind: "kept", text: "[[?: the " },
        { kind: "removed", text: "old " },
        { kind: "kept", text: "date]]" },
      ],
      gaps,
    );
    expect(runs.map((run) => run.gap)).toEqual([0, 0, 0]);
  });

  test("removed words are not part of the proposed text", () => {
    const runs = proofRuns(
      [
        { kind: "removed", text: "gone" },
        { kind: "kept", text: "stays" },
      ],
      [],
    );
    expect(runsText(runs)).toBe("stays");
    expect(runs[0]).toEqual({ kind: "removed", text: "gone", gap: null });
  });
});
