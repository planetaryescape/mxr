/*
 * Which paragraph a selection starts in, so a highlight can be drawn where
 * it was made and not at the first place the same words appear. The plain
 * reader tags its paragraphs `data-paragraph` (the index into the daemon's
 * paragraph list); the HTML reader is numbered by the reader package
 * (`data-para`, one-based), read here through its `paragraphIndexOf`.
 */

import { paragraphIndexOf } from "@planetaryescape/reader";

/** The attribute the plain reader puts on each paragraph. */
export const PARAGRAPH_ATTRIBUTE = "data-paragraph";

const wholeIndex = (value: number | null | undefined): number | null =>
  typeof value === "number" && Number.isInteger(value) && value >= 0 ? value : null;

/** The paragraph index holding `node`, or null when it is in neither reader's text. */
export function paragraphOfNode(node: Node | null): number | null {
  const element = node instanceof Element ? node : (node?.parentElement ?? null);
  const plain = element?.closest<HTMLElement>(`[${PARAGRAPH_ATTRIBUTE}]`);
  if (plain) return wholeIndex(Number(plain.getAttribute(PARAGRAPH_ATTRIBUTE)));
  return wholeIndex(paragraphIndexOf(node));
}

/**
 * The paragraph a range starts in. A range that starts at an element
 * boundary (a triple-click does) starts at the child that offset points to.
 */
export function paragraphOfRangeStart(range: Range): number | null {
  const { startContainer, startOffset } = range;
  const start =
    startContainer.nodeType === Node.ELEMENT_NODE
      ? (startContainer.childNodes[startOffset] ?? startContainer)
      : startContainer;
  return paragraphOfNode(start);
}

/** The paragraph the current selection starts in, if it lies inside `container`. */
export function selectionParagraph(container: Element | null): number | null {
  const selection = window.getSelection();
  if (!container || !selection || selection.rangeCount === 0) return null;
  const range = selection.getRangeAt(0);
  if (!container.contains(range.commonAncestorContainer)) return null;
  return paragraphOfRangeStart(range);
}

/** Paragraphs either reader anchors. */
const ANCHORED = `[${PARAGRAPH_ATTRIBUTE}], [data-para]`;

/**
 * The paragraph at the top of `viewport`, for highlighting from the
 * keyboard: the first anchored paragraph in `container` that is not
 * scrolled wholly above the viewport's top, as the TUI takes the paragraph
 * that owns its first visible line. Null when none is in view (the
 * sender's own layout has no anchors).
 */
export function paragraphAtTop(
  container: Element | null,
  viewport: Element | null,
): { index: number; text: string } | null {
  if (!container || !viewport) return null;
  const { top, bottom } = viewport.getBoundingClientRect();
  for (const element of container.querySelectorAll(ANCHORED)) {
    const box = element.getBoundingClientRect();
    if (box.bottom <= top + 1) continue;
    if (box.top >= bottom) return null;
    const index = paragraphOfNode(element);
    const text = (element.textContent ?? "").replace(/\s+/g, " ").trim();
    if (index !== null && text) return { index, text };
  }
  return null;
}
