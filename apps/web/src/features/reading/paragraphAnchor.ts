import { paragraphIndexOf } from "@planetaryescape/reader";

export const PARAGRAPH_ATTRIBUTE = "data-paragraph";

const wholeIndex = (value: number | null | undefined): number | null =>
  typeof value === "number" && Number.isInteger(value) && value >= 0 ? value : null;

export function paragraphOfNode(node: Node | null): number | null {
  const element = node instanceof Element ? node : (node?.parentElement ?? null);
  const plain = element?.closest<HTMLElement>(`[${PARAGRAPH_ATTRIBUTE}]`);
  if (plain) return wholeIndex(Number(plain.getAttribute(PARAGRAPH_ATTRIBUTE)));
  return wholeIndex(paragraphIndexOf(node));
}

export function paragraphOfRangeStart(range: Range): number | null {
  const { startContainer, startOffset } = range;
  const start =
    startContainer.nodeType === Node.ELEMENT_NODE
      ? (startContainer.childNodes[startOffset] ?? startContainer)
      : startContainer;
  return paragraphOfNode(start);
}

export function selectionParagraph(container: Element | null): number | null {
  const selection = window.getSelection();
  if (!container || !selection || selection.rangeCount === 0) return null;
  const range = selection.getRangeAt(0);
  if (!container.contains(range.commonAncestorContainer)) return null;
  return paragraphOfRangeStart(range);
}

const ANCHORED = `[${PARAGRAPH_ATTRIBUTE}], [data-para]`;

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
