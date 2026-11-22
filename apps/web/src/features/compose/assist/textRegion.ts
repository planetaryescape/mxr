export const FORWARD_HEADER = "---------- Forwarded message ----------";

export interface TextRegion {
  prefix: string;
  text: string;
  suffix: string;
}

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
  textStart: number;
}

function needsBreak(left: string, right: string): boolean {
  return left !== "" && right !== "" && !/\s$/.test(left) && !/^\s/.test(right);
}

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

export function utf8Offset(text: string, index: number): number {
  return new TextEncoder().encode(text.slice(0, index)).length;
}

export function utf16Index(text: string, byteOffset: number): number {
  const bytes = new TextEncoder().encode(text);
  const decoder = new TextDecoder();
  return decoder.decode(bytes.slice(0, Math.max(0, Math.min(byteOffset, bytes.length)))).length;
}
