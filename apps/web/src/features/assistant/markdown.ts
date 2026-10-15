/*
 * The assistant's replies are light markdown. They are model text built
 * from other people's email, so the HTML is sanitized, nothing remote
 * loads (no images: a tracking pixel would tell a sender you looked),
 * web links open outside the app without a referrer, and addresses stay
 * plain text. Links to an email or a person (`mxr:`) open in the app.
 */

import DOMPurify from "dompurify";
import { marked } from "marked";

import { linkHref, parseMxrLink } from "./links";

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
  ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|mxr:)/i,
};

/** Marks a link the transcript opens with the router instead of the browser. */
export const APP_LINK_ATTRIBUTE = "data-app-link";

let hooked = false;
// DOMPurify hooks are global; this is set for the one synchronous sanitize call.
let appLinks = false;

function hookLinks(): void {
  if (hooked) return;
  hooked = true;
  DOMPurify.addHook("afterSanitizeAttributes", (node) => {
    if (node.tagName !== "A") return;
    const href = node.getAttribute("href") ?? "";
    const toText = () =>
      node.replaceWith(node.ownerDocument.createTextNode(node.textContent ?? ""));
    // An address the model mentions stays text: a mailto link would open
    // another mail app from inside this one.
    if (href.toLowerCase().startsWith("mailto:")) {
      toText();
      return;
    }
    if (href.toLowerCase().startsWith("mxr:")) {
      const link = appLinks ? parseMxrLink(href) : null;
      if (!link) {
        toText();
        return;
      }
      node.setAttribute("href", linkHref(link));
      node.setAttribute(APP_LINK_ATTRIBUTE, "");
      return;
    }
    node.setAttribute("target", "_blank");
    node.setAttribute("rel", "noopener noreferrer nofollow");
  });
}

/**
 * Reply markdown as safe HTML. `appLinks` only for a stored reply: the
 * daemon has checked its links against what the tools returned. A reply
 * still being written shows its links as words.
 */
export function renderMarkdown(text: string, { appLinks: inApp = false } = {}): string {
  hookLinks();
  const html = marked.parse(text, { async: false, gfm: true, breaks: true });
  appLinks = inApp;
  try {
    // ADD_ATTR keeps the marker the hook sets after the allow-list ran.
    return DOMPurify.sanitize(html, { ...SANITIZE, ADD_ATTR: [APP_LINK_ATTRIBUTE] });
  } finally {
    appLinks = false;
  }
}
