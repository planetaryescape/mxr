/*
 * Model-written spans stay marked until the user edits them (D136). A span
 * is a UTF-16 range of the body. Edits arrive as whole-body replacements
 * from the editors, so the change is found as the one contiguous region
 * that differs; a span that overlaps it loses its mark, and spans after it
 * move with the text.
 */

export interface ModelSpan {
  start: number;
  end: number;
}

export function mapModelSpans(
  spans: readonly ModelSpan[],
  before: string,
  after: string,
): ModelSpan[] {
  if (before === after) return [...spans];
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
