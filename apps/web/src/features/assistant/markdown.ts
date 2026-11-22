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

export const APP_LINK_ATTRIBUTE = "data-app-link";
export const APP_LINK_ID_ATTRIBUTE = "data-app-link-id";

let hooked = false;
let appLinks = false;

function hookLinks(): void {
  if (hooked) return;
  hooked = true;
  DOMPurify.addHook("afterSanitizeAttributes", (node) => {
    if (node.tagName !== "A") return;
    node.removeAttribute(APP_LINK_ATTRIBUTE);
    node.removeAttribute(APP_LINK_ID_ATTRIBUTE);
    const href = node.getAttribute("href") ?? "";
    const toText = () =>
      node.replaceWith(node.ownerDocument.createTextNode(node.textContent ?? ""));
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
      node.setAttribute(APP_LINK_ATTRIBUTE, link.kind);
      node.setAttribute(
        APP_LINK_ID_ATTRIBUTE,
        link.kind === "thread" ? link.thread_id : link.person_id,
      );
      return;
    }
    node.setAttribute("target", "_blank");
    node.setAttribute("rel", "noopener noreferrer nofollow");
  });
}

export function renderMarkdown(text: string, { appLinks: inApp = false } = {}): string {
  hookLinks();
  const html = marked.parse(text, { async: false, gfm: true, breaks: true });
  appLinks = inApp;
  try {
    return DOMPurify.sanitize(html, {
      ...SANITIZE,
      ADD_ATTR: [APP_LINK_ATTRIBUTE, APP_LINK_ID_ATTRIBUTE],
    });
  } finally {
    appLinks = false;
  }
}
