/*
 * Split a plain-text body into what the sender wrote, what they quoted,
 * and their signature, so the reader can fold the last two (TUI reader
 * mode and `S`). Pure string work; no DOM.
 */

export type SegmentKind = "text" | "quote" | "signature";

export interface Segment {
  kind: SegmentKind;
  text: string;
  lines: number;
}

/** Quotes shorter than this stay inline; longer ones fold. Matches the TUI. */
export const FOLD_QUOTE_MIN_LINES = 4;

const ATTRIBUTION = [
  /^On\b.{4,200}\bwrote:\s*$/i,
  /^Le\b.{4,200}\ba écrit\s*:\s*$/i,
  /^Am\b.{4,200}\bschrieb\b.*:\s*$/i,
  /^El\b.{4,200}\bescribió:\s*$/i,
];
const ORIGINAL_MESSAGE = /^-{2,}\s*(Original Message|Forwarded message)\s*-{2,}\s*$/i;
const OUTLOOK_FROM = /^\*?From:\*?\s+\S/;
const OUTLOOK_FIELD = /^\*?(Sent|Date|To|Cc|Subject):\*?\s/;
const SIGNATURE = /^-- ?$/;
// RFC 3676 asks for four lines; allow some slack, but a long run after
// "--" is body text using it as a divider, not a signature to fold away.
const SIGNATURE_MAX_LINES = 10;

function isQuoted(line: string): boolean {
  return /^\s*>/.test(line);
}

/** An attribution may wrap across two lines ("On Mon, … at 10:02 AM,\nAda <a@b> wrote:"). */
function attributionLength(lines: string[], index: number): number {
  const line = lines[index]?.trim() ?? "";
  if (ATTRIBUTION.some((pattern) => pattern.test(line))) return 1;
  const joined = `${line} ${lines[index + 1]?.trim() ?? ""}`;
  if (/^On\b/i.test(line) && ATTRIBUTION.some((pattern) => pattern.test(joined))) return 2;
  return 0;
}

function isOutlookHeader(lines: string[], index: number): boolean {
  if (!OUTLOOK_FROM.test(lines[index]?.trim() ?? "")) return false;
  let fields = 0;
  for (let offset = 1; offset <= 4; offset += 1) {
    if (OUTLOOK_FIELD.test(lines[index + offset]?.trim() ?? "")) fields += 1;
  }
  return fields >= 2;
}

export function splitMessageText(input: string): Segment[] {
  const lines = input.replace(/\r\n?/g, "\n").split("\n");
  const segments: Segment[] = [];
  let buffer: string[] = [];
  let kind: SegmentKind = "text";

  const flush = () => {
    const text = buffer.join("\n");
    if (text.trim().length > 0) {
      const previous = segments.at(-1);
      if (previous && previous.kind === kind) {
        previous.text = `${previous.text}\n${text}`;
        previous.lines += buffer.length;
      } else {
        segments.push({ kind, text, lines: buffer.length });
      }
    }
    buffer = [];
  };

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index] ?? "";

    // Everything after a quote attribution or an Outlook/forward header is
    // the quoted conversation (top-posting), however it is formatted.
    const attribution = attributionLength(lines, index);
    if (attribution > 0 || ORIGINAL_MESSAGE.test(line.trim()) || isOutlookHeader(lines, index)) {
      flush();
      kind = "quote";
      buffer = lines.slice(index);
      flush();
      break;
    }

    if (SIGNATURE.test(line) && signatureLength(lines, index + 1) <= SIGNATURE_MAX_LINES) {
      flush();
      kind = "signature";
      continue;
    }

    const quoted = isQuoted(line);
    if (kind === "signature") {
      // A signature ends where quoted text starts.
      if (quoted) {
        flush();
        kind = "quote";
      }
      buffer.push(line);
      continue;
    }
    const next: SegmentKind = quoted ? "quote" : "text";
    if (next !== kind) {
      // Blank lines inside a quote block belong to it.
      if (kind === "quote" && line.trim() === "" && isQuoted(lines[index + 1] ?? "")) {
        buffer.push(line);
        continue;
      }
      flush();
      kind = next;
    }
    buffer.push(line);
  }
  flush();

  // Short quotes read better inline than folded.
  for (const segment of segments) {
    if (
      segment.kind === "quote" &&
      segment.lines < FOLD_QUOTE_MIN_LINES &&
      !startsWithAttribution(segment.text)
    ) {
      segment.kind = "text";
    }
  }
  return segments;
}

/** Non-blank lines from `start` until quoted text or the end. */
function signatureLength(lines: string[], start: number): number {
  let count = 0;
  for (let index = start; index < lines.length; index += 1) {
    const line = lines[index] ?? "";
    if (isQuoted(line)) break;
    if (line.trim() !== "") count += 1;
  }
  return count;
}

function startsWithAttribution(text: string): boolean {
  const lines = text.split("\n");
  return (
    attributionLength(lines, 0) > 0 ||
    ORIGINAL_MESSAGE.test(lines[0]?.trim() ?? "") ||
    isOutlookHeader(lines, 0)
  );
}

/** Merge adjacent text segments after the short-quote pass. */
export function normalizeSegments(segments: Segment[]): Segment[] {
  const out: Segment[] = [];
  for (const segment of segments) {
    const previous = out.at(-1);
    if (previous && previous.kind === segment.kind) {
      previous.text = `${previous.text}\n${segment.text}`;
      previous.lines += segment.lines;
    } else {
      out.push({ ...segment });
    }
  }
  return out;
}
