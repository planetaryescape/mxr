export const LINK_MARK_ATTRIBUTE = "data-todo-link";

export interface LinkHighlight {
  messageId?: string;
  url: string;
  title: string;
  domain?: string;
}

function normalLink(value: string): string {
  try {
    const url = new URL(value.trim());
    const path = url.pathname.replace(/\/+$/, "");
    return `${url.protocol}//${url.host.toLowerCase()}${path}${url.search}`;
  } catch {
    return value.trim();
  }
}

export function sameLink(a: string, b: string): boolean {
  return normalLink(a) === normalLink(b);
}

export function markLinkInDocument(root: ParentNode, url: string | undefined): Element | null {
  let first: Element | null = null;
  for (const anchor of root.querySelectorAll("a[href]")) {
    const href = anchor.getAttribute("href") ?? "";
    if (url && sameLink(href, url)) {
      anchor.setAttribute(LINK_MARK_ATTRIBUTE, "");
      first ??= anchor;
    } else {
      anchor.removeAttribute(LINK_MARK_ATTRIBUTE);
    }
  }
  return first;
}
