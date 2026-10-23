/*
 * Model-written spans stay marked until the user edits them (D136). A span
 * is a UTF-16 range of the body. A user's typed edit arrives as a whole-body
 * replacement, so the one contiguous region that differs is found and any
 * span overlapping it loses its mark. An accepted proof carries its marks
 * through its own kept runs instead (carryModelSpans): a sentence the model
 * left alone keeps its mark even when the proof adds text around it.
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
export function addedSpans(
  pieces: readonly ProofPiece[],
  textStart: number,
  limit: number = Number.POSITIVE_INFINITY,
): ModelSpan[] {
  const spans: ModelSpan[] = [];
  let offset = 0;
  for (const piece of pieces) {
    if (piece.kind === "removed") continue;
    if (piece.kind === "added") {
      // The insert drops trailing blank space, so a run past `limit` is clipped.
      const start = Math.min(offset, limit);
      const end = Math.min(offset + piece.text.length, limit);
      if (end > start) spans.push({ start: textStart + start, end: textStart + end });
    }
    offset += piece.text.length;
  }
  return spans;
}

/** A run the proof kept: where it sits in the old text and in the new one. */
export interface KeptRun {
  /** Offset in the user's text before the accept. */
  oldStart: number;
  length: number;
  /** Offset in the proof's text. */
  newStart: number;
}

/**
 * The kept runs of a proof, given the two texts they must spell. Returns null
 * when the changes do not account for both texts exactly; the caller then
 * falls back to the whole-body mapping rather than trust a bad diff.
 */
export function keptRuns(
  changes: readonly { kind: string; text: string }[],
  oldText: string,
  newText: string,
): KeptRun[] | null {
  const runs: KeptRun[] = [];
  let oldSpelled = "";
  let newSpelled = "";
  for (const change of changes) {
    if (change.kind === "kept") {
      runs.push({
        oldStart: oldSpelled.length,
        length: change.text.length,
        newStart: newSpelled.length,
      });
      oldSpelled += change.text;
      newSpelled += change.text;
    } else if (change.kind === "removed") {
      oldSpelled += change.text;
    } else if (change.kind === "added") {
      newSpelled += change.text;
    } else {
      return null;
    }
  }
  return oldSpelled === oldText && newSpelled === newText ? runs : null;
}

/** Where the user's text sat before the accept and where the proof put it. */
export interface ProofLayout {
  oldRegionStart: number;
  oldRegionEnd: number;
  newRegionStart: number;
  /** Length of the inserted text, without the blank space the insert drops. */
  insertedLength: number;
  /** How far the signature and whatever follows the text moved. */
  suffixShift: number;
}

/**
 * Carry marks through an accepted proof. A mark keeps the characters the proof
 * kept and is cleared only where its own text was removed: a span over kept
 * text moves with it, and a span across an edit keeps its unchanged parts.
 * Text outside the user's region moves with the region's end.
 */
export function carryModelSpans(
  spans: readonly ModelSpan[],
  layout: ProofLayout,
  kept: readonly KeptRun[],
): ModelSpan[] {
  const carried: ModelSpan[] = [];
  const push = (start: number, end: number) => {
    if (end > start) carried.push({ start, end });
  };
  for (const span of spans) {
    push(span.start, Math.min(span.end, layout.oldRegionStart));
    push(
      Math.max(span.start, layout.oldRegionEnd) + layout.suffixShift,
      span.end + layout.suffixShift,
    );
    const from = Math.max(span.start, layout.oldRegionStart) - layout.oldRegionStart;
    const to = Math.min(span.end, layout.oldRegionEnd) - layout.oldRegionStart;
    for (const run of kept) {
      const runEnd = run.oldStart + run.length;
      const start = Math.max(from, run.oldStart);
      const end = Math.min(to, runEnd);
      if (end <= start) continue;
      const newStart = Math.min(start - run.oldStart + run.newStart, layout.insertedLength);
      const newEnd = Math.min(end - run.oldStart + run.newStart, layout.insertedLength);
      push(layout.newRegionStart + newStart, layout.newRegionStart + newEnd);
    }
  }
  return carried;
}
