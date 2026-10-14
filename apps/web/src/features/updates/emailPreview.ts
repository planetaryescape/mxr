/*
 * The first few lines of an update's email, for its peek: enough to read
 * what it says without opening it. Plain text only. The daemon's reader
 * text is already cleaned of markup, and the lines are shown as text nodes,
 * never parsed, so nothing in a message can add elements to the page.
 */

import type { MessageBodyView } from "@/features/mailbox/types";

/** Lines shown, and the longest one, in characters. */
export const PREVIEW_LINES = 4;
const LINE_MAX_CHARS = 160;
/** How far into the message to look for informative lines. */
const SCAN_LINES = 80;

const BARE_LINK = /^(?:\[[^\]]*\]\s*)?<?(?:https?:\/\/|www\.)\S+>?$/i;
const IMAGE_PLACEHOLDER = /^\[(?:image|img|cid)[^\]]*\]$/i;
const BOILERPLATE =
  /\b(?:view (?:this )?(?:email )?in (?:your )?browser|unsubscribe|manage (?:your )?(?:email )?preferences|privacy policy|terms of (?:service|use)|all rights reserved|you(?:'re| are) receiving this|do not reply|this is an automated)\b|^©/i;
const GREETING = /^(?:hi|hello|hey|dear)\b[^.!?]{0,40}[,!]?$/i;

function clip(line: string): string {
  if (line.length <= LINE_MAX_CHARS) return line;
  const cut = line.slice(0, LINE_MAX_CHARS - 1);
  const word = cut.lastIndexOf(" ");
  return `${(word > LINE_MAX_CHARS / 2 ? cut.slice(0, word) : cut).trimEnd()}…`;
}

/** Whether a line says something: not a greeting, footer, bare link or fragment. */
function informative(line: string): boolean {
  if (!/[\p{L}\p{N}]/u.test(line)) return false;
  if (BARE_LINK.test(line) || IMAGE_PLACEHOLDER.test(line)) return false;
  if (BOILERPLATE.test(line) || GREETING.test(line)) return false;
  // A receipt's "Total $24.00" is two words; a lone word is a label.
  return line.split(" ").length >= 2 || /\d/.test(line);
}

/** The first informative lines of a message's text, whitespace collapsed. */
export function previewLines(text: string | null | undefined): string[] {
  if (typeof text !== "string") return [];
  const out: string[] = [];
  for (const raw of text.split(/\r?\n/).slice(0, SCAN_LINES)) {
    const line = raw
      .replace(/^[\s>*\-#|]+/, "")
      .replace(/\s+/g, " ")
      .trim();
    if (!line || !informative(line) || line === out.at(-1)) continue;
    out.push(clip(line));
    if (out.length === PREVIEW_LINES) break;
  }
  return out;
}

/** The text the reader shows for a message: its cleaned reader text, else the plain part. */
export function previewSource(body: MessageBodyView | undefined): string | null {
  return body?.reader_text || body?.text_plain || null;
}
