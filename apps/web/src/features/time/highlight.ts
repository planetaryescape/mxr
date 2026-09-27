import type { TimeSpan } from "./api";

export interface HighlightSegment {
  /** Index of the segment's first character in the text; unique per run. */
  start: number;
  text: string;
  understood: boolean;
}

/**
 * The daemon reports spans as UTF-8 byte offsets (Rust string indices);
 * JavaScript strings index UTF-16 code units. Walk code points to convert.
 */
export function byteOffsetToIndex(text: string, byteOffset: number): number {
  let bytes = 0;
  let index = 0;
  while (index < text.length && bytes < byteOffset) {
    const codePoint = text.codePointAt(index) ?? 0;
    bytes += codePoint < 0x80 ? 1 : codePoint < 0x800 ? 2 : codePoint < 0x10000 ? 3 : 4;
    index += codePoint > 0xffff ? 2 : 1;
  }
  return index;
}

/** Split `text` into runs the parser understood and runs it didn't. */
export function highlightSegments(text: string, spans: readonly TimeSpan[]): HighlightSegment[] {
  const ranges = spans
    .map((span) => ({
      start: byteOffsetToIndex(text, span.start),
      end: byteOffsetToIndex(text, span.end),
    }))
    .filter((range) => range.end > range.start)
    .toSorted((a, b) => a.start - b.start);
  const segments: HighlightSegment[] = [];
  let cursor = 0;
  for (const range of ranges) {
    if (range.start < cursor) continue;
    if (range.start > cursor) {
      segments.push({ start: cursor, text: text.slice(cursor, range.start), understood: false });
    }
    segments.push({
      start: range.start,
      text: text.slice(range.start, range.end),
      understood: true,
    });
    cursor = range.end;
  }
  if (cursor < text.length) {
    segments.push({ start: cursor, text: text.slice(cursor), understood: false });
  }
  return segments;
}
