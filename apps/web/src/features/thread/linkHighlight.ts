/*
 * The link a To do row is about, highlighted where it sits in its email.
 * A to-do never opens an external link itself: Enter opens the email in
 * the reader, and the reader marks the link so you can read around it
 * before you choose to follow it.
 */

/** Attribute on the highlighted link in every view; the reader scrolls to it. */
export const LINK_MARK_ATTRIBUTE = "data-todo-link";

/** Carried in the reader's URL (`/todo/$threadId?link=…`), so it lasts exactly as long as that opening. */
export interface LinkHighlight {
  /** The message holding the link; the newest when unknown. */
  messageId?: string;
  url: string;
  /** The to-do's title, "Pay council tax", for the line above the email. */
  title: string;
  domain?: string;
}

/**
 * Two links are the same when they go to the same place: scheme and host
 * ignore case, and a trailing slash on the path doesn't count. Anything
 * that isn't a URL only matches itself.
 */
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

/**
 * Mark every anchor in `root` that goes to `url`, and unmark the rest.
 * Returns the first marked anchor, or null when the link isn't there.
 */
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
