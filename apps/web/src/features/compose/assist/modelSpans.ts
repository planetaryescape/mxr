/*
 * Model-written spans stay marked until the user edits them (D136). A span
 * is a UTF-16 range of the body. Edits arrive as whole-body replacements
 * from the editors, so the change is found as the one contiguous region
 * that differs; a span that overlaps it loses its mark, and spans after it
 * move with the text.
 */

import type { ProofPiece } from "./proof";

export interface ModelSpan {
  start: number;
  end: number;
}

/** One shared empty list, so an unmarked draft never allocates per keystroke. */
export const NO_SPANS: readonly ModelSpan[] = [];

export function mapModelSpans(
  spans: readonly ModelSpan[],
  before: string,
  after: string,
): readonly ModelSpan[] {
  if (spans.length === 0 || before === after) return spans;
  const shortest = Math.min(before.length, after.length);
  let prefix = 0;
  while (prefix < shortest && before[prefix] === after[prefix]) prefix++;
  let suffix = 0;
  while (
    suffix < shortest - prefix &&
    before[before.length - 1 - suffix] === after[after.length - 1 - suffix]
  ) {
    suffix++;
  }
  const changedEnd = before.length - suffix;
  const delta = after.length - before.length;
  const mapped: ModelSpan[] = [];
  for (const span of spans) {
    if (span.end <= prefix) mapped.push(span);
    else if (span.start >= changedEnd)
      mapped.push({ start: span.start + delta, end: span.end + delta });
    // Otherwise the edit touched the span: the mark clears.
  }
  return mapped;
}

/**
 * The marks for the words a proof added, placed at `textStart` in the body.
 * Only added runs are marked: the user's own words, and gaps the user still
 * has to fill, are not model prose to mark.
 */
export function addedSpans(pieces: readonly ProofPiece[], textStart: number): ModelSpan[] {
  const spans: ModelSpan[] = [];
  let offset = 0;
  for (const piece of pieces) {
    if (piece.kind === "removed") continue;
    if (piece.kind === "added" && piece.text !== "") {
      spans.push({ start: textStart + offset, end: textStart + offset + piece.text.length });
    }
    offset += piece.text.length;
  }
  return spans;
}
