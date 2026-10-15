/*
 * The assistant's replies are light markdown. They are model text built
 * from other people's email, so the HTML is sanitized, nothing remote
 * loads (no images: a tracking pixel would tell a sender you looked),
 * web links open outside the app without a referrer, and addresses stay
 * plain text.
 */

import DOMPurify from "dompurify";
import { marked } from "marked";

const SANITIZE = {
  ALLOWED_TAGS: [
    "p",
    "br",
    "strong",
    "em",
    "code",
    "pre",
    "ul",
    "ol",
    "li",
    "blockquote",
    "a",
    "h1",
    "h2",
    "h3",
    "h4",
    "hr",
    "table",
    "thead",
    "tbody",
    "tr",
    "th",
    "td",
    "del",
  ],
  ALLOWED_ATTR: ["href", "title"],
  ALLOWED_URI_REGEXP: /^(?:https?:|mailto:)/i,
};

let hooked = false;

function hookLinks(): void {
  if (hooked) return;
  hooked = true;
  DOMPurify.addHook("afterSanitizeAttributes", (node) => {
    // An address the model mentions stays text: a mailto link would open
    // another mail app from inside this one.
    if (node.tagName === "A" && node.getAttribute("href")?.toLowerCase().startsWith("mailto:")) {
      node.replaceWith(node.ownerDocument.createTextNode(node.textContent ?? ""));
      return;
    }
    if (node.tagName === "A") {
      node.setAttribute("target", "_blank");
      node.setAttribute("rel", "noopener noreferrer nofollow");
    }
  });
}

export function renderMarkdown(text: string): string {
  hookLinks();
  const html = marked.parse(text, { async: false, gfm: true, breaks: true });
  return DOMPurify.sanitize(html, SANITIZE);
}
