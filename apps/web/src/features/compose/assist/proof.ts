/*
 * A proof as the page draws it: the daemon's change list cut into pieces
 * that say what each run of words is. Kept words are the user's own; added
 * words came from the model; removed words are struck; gaps are the
 * `[[?: …]]` spots the user still has to fill. Gap offsets are UTF-8 bytes
 * into the proposed text, like every other offset on this contract.
 */

import type { ComposeAssistChange, ComposeAssistGap } from "./types";

export type ProofPieceKind = "kept" | "added" | "removed" | "gap";

export interface ProofPiece {
  kind: ProofPieceKind;
  text: string;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export function proofPieces(
  changes: readonly ComposeAssistChange[],
  gaps: readonly ComposeAssistGap[],
): ProofPiece[] {
  const pieces: ProofPiece[] = [];
  let offset = 0;
  for (const change of changes) {
    if (change.kind === "removed") {
      if (change.text) pieces.push({ kind: "removed", text: change.text });
      continue;
    }
    const bytes = encoder.encode(change.text);
    const segmentStart = offset;
    const segmentEnd = offset + bytes.length;
    offset = segmentEnd;

    // Cut the segment at every gap edge, so each run is wholly inside or
    // wholly outside a gap.
    const cuts = new Set<number>([segmentStart, segmentEnd]);
    for (const gap of gaps) {
      if (gap.start > segmentStart && gap.start < segmentEnd) cuts.add(gap.start);
      if (gap.end > segmentStart && gap.end < segmentEnd) cuts.add(gap.end);
    }
    const sorted = [...cuts].toSorted((a, b) => a - b);
    for (let index = 0; index < sorted.length - 1; index++) {
      const from = sorted[index] ?? segmentStart;
      const to = sorted[index + 1] ?? segmentEnd;
      const inGap = gaps.some((gap) => gap.start <= from && to <= gap.end);
      const text = decoder.decode(bytes.slice(from - segmentStart, to - segmentStart));
      if (text === "") continue;
      pieces.push({ kind: inGap ? "gap" : change.kind, text });
    }
  }
  return pieces;
}

/** The proposed text the pieces spell, which must equal `proposal.text`. */
export function piecesText(pieces: readonly ProofPiece[]): string {
  return pieces
    .filter((piece) => piece.kind !== "removed")
    .map((piece) => piece.text)
    .join("");
}
