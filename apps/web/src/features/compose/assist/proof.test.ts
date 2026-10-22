import { describe, expect, test } from "vitest";

import { piecesText, proofPieces } from "./proof";

describe("proofPieces", () => {
  test("kept, added, removed and gap runs come out in order", () => {
    const changes = [
      { kind: "kept" as const, text: "Thursday works. " },
      { kind: "removed" as const, text: "numbers" },
      { kind: "added" as const, text: "Numbers" },
      { kind: "kept" as const, text: " by " },
      { kind: "added" as const, text: "[[?: a time]]" },
    ];
    const proposalText = "Thursday works. Numbers by [[?: a time]]";
    const gapStart = utf8Length(proposalText.slice(0, proposalText.indexOf("[[?")));
    const gaps = [
      { label: "a time", start: gapStart, end: gapStart + utf8Length("[[?: a time]]") },
    ];
    const pieces = proofPieces(changes, gaps);
    expect(pieces.map((piece) => piece.kind)).toEqual(["kept", "removed", "added", "kept", "gap"]);
    expect(piecesText(pieces)).toBe(proposalText);
  });

  test("a gap inside an added run splits it", () => {
    const text = "Any time after [[?: a time]] suits me";
    const start = utf8Length("Any time after ");
    const end = start + utf8Length("[[?: a time]]");
    const pieces = proofPieces([{ kind: "added", text }], [{ label: "a time", start, end }]);
    expect(pieces).toEqual([
      { kind: "added", text: "Any time after " },
      { kind: "gap", text: "[[?: a time]]" },
      { kind: "added", text: " suits me" },
    ]);
    expect(piecesText(pieces)).toBe(text);
  });

  test("multi-byte text keeps its gap offsets in bytes", () => {
    const text = "café [[?: when]] ok";
    const start = utf8Length("café ");
    const end = start + utf8Length("[[?: when]]");
    const pieces = proofPieces([{ kind: "kept", text }], [{ label: "when", start, end }]);
    expect(pieces.find((piece) => piece.kind === "gap")?.text).toBe("[[?: when]]");
  });

  test("removed words are not part of the proposed text", () => {
    const pieces = proofPieces(
      [
        { kind: "removed", text: "gone" },
        { kind: "kept", text: "stays" },
      ],
      [],
    );
    expect(piecesText(pieces)).toBe("stays");
    expect(pieces[0]).toEqual({ kind: "removed", text: "gone" });
  });
});

function utf8Length(text: string): number {
  return new TextEncoder().encode(text).length;
}
