/*
 * The part of a draft the user wrote: the body without the forward header
 * above it and the signature block below it. Everything outside the region
 * is kept verbatim, so an accepted proof never touches the signature.
 */

export const FORWARD_HEADER = "---------- Forwarded message ----------";

export interface TextRegion {
  /** Body before the region: a forward header, or a signature daemon put on top. */
  prefix: string;
  /** The user's text, trimmed at both ends. */
  text: string;
  /** Body after the region: the signature block. */
  suffix: string;
}

/**
 * The signature delimiter is `-- ` or `--` on a line of its own, the same
 * line `signature_block` writes in crates/compose/src/lib.rs. The last one
 * that is not inside a forward header marks the block; a body with no such
 * line has no signature.
 */
const SIGNATURE_DELIMITER = /^--[ \t]?$/;

function lineStarts(body: string): number[] {
  const starts = [0];
  for (let i = 0; i < body.length; i++) if (body[i] === "\n") starts.push(i + 1);
  return starts;
}

function lastSignatureLine(body: string): number {
  const starts = lineStarts(body);
  for (let index = starts.length - 1; index >= 0; index--) {
    const start = starts[index] ?? 0;
    const end = body.indexOf("\n", start);
    const line = body.slice(start, end === -1 ? body.length : end);
    if (SIGNATURE_DELIMITER.test(line)) return start;
  }
  return -1;
}

export function splitTextRegion(body: string): TextRegion {
  let start = 0;
  let end = body.length;

  const headerAt = body.indexOf(FORWARD_HEADER);
  const signatureAt = lastSignatureLine(body);
  // A signature below the forward header ends the region; one above it
  // (the daemon puts a forward's signature on top) belongs to the prefix.
  if (signatureAt > (headerAt === -1 ? -1 : headerAt)) end = signatureAt;

  if (headerAt !== -1 && headerAt < end) {
    const afterHeader = headerAt + FORWARD_HEADER.length;
    start = afterHeader;
    while (start < end && (body[start] === "\n" || body[start] === "\r")) start++;
  }

  const raw = body.slice(start, end);
  const trimmedStart = raw.length - raw.trimStart().length;
  const text = raw.trim();
  const regionStart = start + trimmedStart;
  const regionEnd = regionStart + text.length;
  return {
    prefix: body.slice(0, regionStart),
    text,
    suffix: body.slice(regionEnd),
  };
}

/**
 * Put a new text back between the prefix and suffix. The separators are
 * re-created as blank lines, so a round trip through an unchanged region is
 * exact only when the body already used blank lines there. `textStart` is
 * where the text begins in the returned body, in UTF-16 units.
 */
export function joinTextRegion(
  region: TextRegion,
  text: string,
): { body: string; textStart: number } {
  const parts = [region.prefix.trimEnd(), text.trim(), region.suffix.trimStart()];
  const kept = parts.map((part, index) => ({ part, index })).filter(({ part }) => part !== "");
  const body = kept.map(({ part }) => part).join("\n\n");
  const before = kept.find(({ index }) => index === 1);
  const prefixPart = kept.find(({ index }) => index === 0);
  const textStart = before ? (prefixPart ? prefixPart.part.length + 2 : 0) : -1;
  return { body, textStart };
}

/** UTF-8 byte offset of a UTF-16 index into `text`. */
export function utf8Offset(text: string, index: number): number {
  return new TextEncoder().encode(text.slice(0, index)).length;
}

/** UTF-16 index into `text` of a UTF-8 byte offset (clamped to the text). */
export function utf16Index(text: string, byteOffset: number): number {
  const bytes = new TextEncoder().encode(text);
  const decoder = new TextDecoder();
  return decoder.decode(bytes.slice(0, Math.max(0, Math.min(byteOffset, bytes.length)))).length;
}
