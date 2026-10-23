/*
 * The part of a draft the user wrote. The cut is the daemon's `split_body`
 * (crates/composer/src/check/regions.rs): the text ends at the first line that
 * is `--` once trailing space is ignored, or that starts with the forward
 * header. Everything from that line on is kept byte for byte, so a signature
 * or a forwarded message never reaches the model or changes on accept.
 * Leading blank lines stay in the prefix. Offsets are UTF-8 bytes of the text
 * region, as the contract says.
 */

export const FORWARD_HEADER = "---------- Forwarded message ----------";

export interface TextRegion {
  /** Leading blank lines, kept as they are. */
  prefix: string;
  /** The user's text: no leading or trailing blank space. */
  text: string;
  /** The blank space after the text, then the signature or forward, byte for byte. */
  suffix: string;
}

/** UTF-16 offset of the first line that ends the text, or the body's length. */
export function cutOffset(body: string): number {
  let offset = 0;
  for (;;) {
    const newline = body.indexOf("\n", offset);
    const end = newline === -1 ? body.length : newline;
    const bare = body.slice(offset, end).trimEnd();
    if (bare === "--" || bare.startsWith(FORWARD_HEADER)) return offset;
    if (newline === -1) return body.length;
    offset = newline + 1;
  }
}

export function splitTextRegion(body: string): TextRegion {
  const before = body.slice(0, cutOffset(body));
  const head = before.trimStart();
  const start = before.length - head.length;
  const text = head.trimEnd();
  return {
    prefix: body.slice(0, start),
    text,
    suffix: body.slice(start + text.length),
  };
}

export interface JoinedRegion {
  body: string;
  /** UTF-16 index where the inserted text starts in `body`. */
  textStart: number;
}

/** A break is needed where two non-blank pieces meet with no whitespace between. */
function needsBreak(left: string, right: string): boolean {
  return left !== "" && right !== "" && !/\s$/.test(left) && !/^\s/.test(right);
}

/**
 * The body with `text` in place of the user's text. Trailing blank space in
 * `text` is dropped, as the daemon's `with_text` does; the suffix keeps its
 * own spacing, and a blank-line break is added only where two pieces would
 * otherwise run together.
 */
export function joinTextRegion(region: TextRegion, text: string): JoinedRegion {
  const { prefix, suffix } = region;
  const inserted = text.trimEnd();
  if (inserted === "") {
    const separator = needsBreak(prefix, suffix) ? "\n\n" : "";
    return { body: prefix + separator + suffix, textStart: prefix.length };
  }
  const beforeText = needsBreak(prefix, inserted) ? "\n\n" : "";
  const afterText = needsBreak(inserted, suffix) ? "\n\n" : "";
  const textStart = prefix.length + beforeText.length;
  return {
    body: prefix + beforeText + inserted + afterText + suffix,
    textStart,
  };
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
