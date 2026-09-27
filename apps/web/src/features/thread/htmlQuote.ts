/*
 * Find the quoted conversation and signature inside an HTML body, using
 * the markers the big clients leave: Gmail's `gmail_quote` and
 * `gmail_signature`, Apple Mail's `blockquote type=cite`, Outlook's
 * `divRplyFwdMsg` / `appendonsend`, Yahoo's `yahoo_quoted`. The reader
 * renders the body without them until the user asks.
 */

const QUOTE_SELECTORS = [
  ".gmail_quote",
  "blockquote[type='cite']",
  "#appendonsend",
  "div[id^='divRplyFwdMsg']",
  "#divRplyFwdMsg",
  ".yahoo_quoted",
  "#mail-editor-reference-message-container",
  ".moz-cite-prefix",
  "#reply-intro",
];

const SIGNATURE_SELECTORS = [
  ".gmail_signature",
  "[data-smartmail='gmail_signature']",
  "#Signature",
  ".moz-signature",
];

export interface HtmlParts {
  /** The body with quotes and signatures removed. */
  main: string;
  hasQuote: boolean;
  hasSignature: boolean;
}

export function splitHtmlQuote(html: string, options: { keepSignature?: boolean } = {}): HtmlParts {
  if (typeof DOMParser === "undefined") return { main: html, hasQuote: false, hasSignature: false };
  const doc = new DOMParser().parseFromString(html, "text/html");
  const body = doc.body;
  let hasQuote = false;
  let hasSignature = false;

  for (const selector of SIGNATURE_SELECTORS) {
    for (const node of Array.from(body.querySelectorAll(selector))) {
      hasSignature = true;
      if (!options.keepSignature) node.remove();
    }
  }

  const first = firstInDocumentOrder(body, QUOTE_SELECTORS);
  if (first) {
    hasQuote = true;
    // Outlook separates the reply with an <hr> or a header block, then puts
    // the whole prior conversation after it: drop the marker and everything
    // that follows at its level and above.
    removeFrom(first);
  }

  // A body that is nothing but quote (a bare forward) keeps its quote.
  if (hasQuote && (body.textContent ?? "").trim().length === 0 && !body.querySelector("img")) {
    return { main: html, hasQuote: false, hasSignature };
  }
  return { main: body.innerHTML, hasQuote, hasSignature };
}

function firstInDocumentOrder(root: Element, selectors: string[]): Element | null {
  let first: Element | null = null;
  for (const selector of selectors) {
    const match = root.querySelector(selector);
    if (!match) continue;
    if (!first || first.compareDocumentPosition(match) & Node.DOCUMENT_POSITION_PRECEDING) {
      first = match;
    }
  }
  return first;
}

/** Remove `node`, then every later sibling of it and of each ancestor. */
function removeFrom(node: Element): void {
  let current: Node | null = node;
  while (current && current.parentNode && current.nodeName !== "BODY") {
    let sibling = current.nextSibling;
    while (sibling) {
      const next: ChildNode | null = sibling.nextSibling;
      sibling.remove();
      sibling = next;
    }
    const parent: Node | null = current.parentNode;
    if (current === node) (current as Element).remove();
    current = parent;
  }
}

/** Count remote images a body would load (trackers excluded by the sanitizer). */
export function countRemoteImages(html: string): number {
  if (typeof DOMParser === "undefined") return 0;
  const doc = new DOMParser().parseFromString(html, "text/html");
  return Array.from(doc.querySelectorAll("img[src]")).filter((image) =>
    /^https?:\/\//i.test(image.getAttribute("src") ?? ""),
  ).length;
}
