import type { ProofRun } from "./proof";

export interface ModelSpan {
  start: number;
  end: number;
}

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
  }
  return mapped;
}

export function addedSpans(
  pieces: readonly ProofRun[],
  textStart: number,
  limit: number = Number.POSITIVE_INFINITY,
): ModelSpan[] {
  const spans: ModelSpan[] = [];
  let offset = 0;
  for (const piece of pieces) {
    if (piece.kind === "removed") continue;
    if (piece.kind === "added") {
      const start = Math.min(offset, limit);
      const end = Math.min(offset + piece.text.length, limit);
      if (end > start) spans.push({ start: textStart + start, end: textStart + end });
    }
    offset += piece.text.length;
  }
  return spans;
}

export interface KeptRun {
  oldStart: number;
  length: number;
  newStart: number;
}

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

export interface ProofLayout {
  oldRegionStart: number;
  oldRegionEnd: number;
  newRegionStart: number;
  insertedLength: number;
  suffixShift: number;
}

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
