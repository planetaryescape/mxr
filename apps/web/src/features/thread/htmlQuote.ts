const QUOTE_SELECTORS = [
  ".gmail_quote",
  "blockquote[type='cite']",
  ".yahoo_quoted",
  ".moz-cite-prefix",
];

const QUOTE_HEADER_SELECTORS = [
  "#appendonsend",
  "div[id^='divRplyFwdMsg']",
  "#divRplyFwdMsg",
  "#mail-editor-reference-message-container",
  "#reply-intro",
];

const SIGNATURE_SELECTORS = [
  ".gmail_signature",
  "[data-smartmail='gmail_signature']",
  "#Signature",
  ".moz-signature",
];

export interface HtmlParts {
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

  const header = firstInDocumentOrder(body, QUOTE_HEADER_SELECTORS);
  if (header) {
    hasQuote = true;
    removeFrom(header);
  }
  for (const selector of QUOTE_SELECTORS) {
    for (const node of Array.from(body.querySelectorAll(selector))) {
      hasQuote = true;
      node.remove();
    }
  }

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
