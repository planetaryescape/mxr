/*
 * Find the ask's quote inside a message so the reader can mark it. The
 * daemon has already checked the quote is in the message; these helpers
 * only locate it in what the reader renders, where line breaks and quote
 * marks may differ. Nothing here parses HTML: the HTML view is marked by
 * walking text nodes of the already-sanitized frame document.
 */

export interface TextRange {
  start: number;
  end: number;
}

/** Attribute that marks the ask in every view; the reader scrolls to it. */
export const ASK_MARK_ATTRIBUTE = "data-ask-quote";

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Straight and curly quotes match each other; everything else is literal. */
function tokenPattern(token: string): string {
  return escapeRegExp(token)
    .replace(/['‘’]/g, "['‘’]")
    .replace(/["“”]/g, '["“”]');
}

/**
 * Locate `quote` in `text`, ignoring how whitespace is laid out. With
 * `gaps: "optional"`, words may also touch, for text joined from separate
 * HTML nodes ("confirm</p><p>who").
 */
export function findQuote(
  text: string,
  quote: string,
  gaps: "required" | "optional" = "required",
): TextRange | null {
  const tokens = quote.trim().split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return null;
  const separator = gaps === "required" ? "\\s+" : "\\s*";
  const match = new RegExp(tokens.map(tokenPattern).join(separator)).exec(text);
  return match ? { start: match.index, end: match.index + match[0].length } : null;
}

/**
 * Wrap the quote in `<mark data-ask-quote>` inside `root`, one mark per text
 * node it spans. Returns the first mark, or null when the quote isn't there.
 * Existing marks are removed first, so calling again with a new quote moves
 * the highlight.
 */
export function markQuoteInDocument(root: HTMLElement, quote: string): HTMLElement | null {
  clearQuoteMarks(root);
  const doc = root.ownerDocument;
  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const nodes: { node: Text; start: number }[] = [];
  let text = "";
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!isText(node)) continue;
    nodes.push({ node, start: text.length });
    text += node.data;
  }
  const range = findQuote(text, quote, "optional");
  if (!range) return null;

  let first: HTMLElement | null = null;
  for (const { node, start } of nodes) {
    const end = start + node.data.length;
    const from = Math.max(range.start, start) - start;
    const to = Math.min(range.end, end) - start;
    if (from >= to) continue;
    const span = doc.createRange();
    span.setStart(node, from);
    span.setEnd(node, to);
    const mark = doc.createElement("mark");
    mark.setAttribute(ASK_MARK_ATTRIBUTE, "");
    span.surroundContents(mark);
    first ??= mark;
  }
  return first;
}

/* Frame documents have their own `Text` constructor, so `instanceof` fails. */
function isText(node: Node): node is Text {
  return node.nodeType === Node.TEXT_NODE;
}

function clearQuoteMarks(root: HTMLElement): void {
  for (const mark of Array.from(root.querySelectorAll(`mark[${ASK_MARK_ATTRIBUTE}]`))) {
    mark.replaceWith(...Array.from(mark.childNodes));
  }
  root.normalize();
}
