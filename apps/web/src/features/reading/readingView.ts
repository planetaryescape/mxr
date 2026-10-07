/*
 * Pure helpers behind the edition and the reader: the cursor order over
 * items and their digest links, how many links show, minutes left at the
 * reader's pace, what `h` saves and what "let go of all" covers. No React,
 * so the rules are tested on their own.
 */

import type {
  ReadingBand,
  ReadingEdition,
  ReadingItem,
  ReadingLink,
  ReadingUnsubscribe,
} from "./api";

/** A digest shows this many links before "+ N more". */
export const LINKS_SHOWN = 4;
/** The longest passage a highlight keeps (the daemon's limit). */
export const MAX_QUOTE_CHARS = 4000;

export type EditionEntry =
  | { kind: "item"; key: string; item: ReadingItem; band: ReadingBand["band"] }
  | {
      kind: "link";
      key: string;
      link: ReadingLink;
      parent: ReadingItem;
      band: ReadingBand["band"];
    };

/** The links a digest shows, and how many more it has. */
export function shownLinks(
  links: readonly ReadingLink[],
  expanded: boolean,
): { shown: readonly ReadingLink[]; more: number } {
  if (expanded || links.length <= LINKS_SHOWN) return { shown: links, more: 0 };
  return { shown: links.slice(0, LINKS_SHOWN), more: links.length - LINKS_SHOWN };
}

/** The bands with items let go of (in flight) left out, and empty bands dropped. */
export function visibleBands(
  edition: ReadingEdition,
  hiddenThreads: ReadonlySet<string>,
): ReadingBand[] {
  return edition.bands
    .map((band) => ({
      ...band,
      items: band.items.filter((item) => !hiddenThreads.has(item.thread_id)),
    }))
    .filter((band) => band.items.length > 0);
}

/** Every focusable entry, in the order the edition shows them. */
export function editionEntries(
  bands: readonly ReadingBand[],
  expanded: ReadonlySet<string>,
): EditionEntry[] {
  const entries: EditionEntry[] = [];
  for (const band of bands) {
    for (const item of band.items) {
      entries.push({ kind: "item", key: item.item_key, item, band: band.band });
      const { shown } = shownLinks(item.links ?? [], expanded.has(item.item_key));
      for (const link of shown) {
        entries.push({ kind: "link", key: link.item_key, link, parent: item, band: band.band });
      }
    }
  }
  return entries;
}

/** "sqlite.org", or "via substack.com" when a click tracker hides the site. */
export function domainLabel(domain: string | null | undefined, tracked?: boolean): string {
  if (!domain) return "";
  return tracked ? `via ${domain}` : domain;
}

/** Whole minutes left of `words` after `progress` (0 to 1), never 0 until done. */
export function minutesLeft(words: number, progress: number, wpm: number): number {
  const left = Math.round(words * (1 - Math.min(1, Math.max(0, progress))));
  if (left <= 0) return 0;
  return Math.max(1, Math.ceil(left / Math.max(1, wpm)));
}

/** How far down a scrolling column is, 0 to 1. A column that fits is read. */
export function scrollProgress(scrollTop: number, scrollHeight: number, clientHeight: number) {
  const room = scrollHeight - clientHeight;
  if (room <= 0) return 1;
  return Math.min(1, Math.max(0, scrollTop / room));
}

/** How unsubscribing reaches the sender, for the preview. */
export function unsubscribeMethodLine(method: ReadingUnsubscribe): string {
  switch (method) {
    case "one_click":
      return "One click: the sender is told directly, no browser.";
    case "link":
      return "A link: mxr opens the sender's unsubscribe page.";
    case "mailto":
      return "An email: mxr sends the list's unsubscribe message.";
    default:
      return "No unsubscribe method: mxr can only let go of its issues here.";
  }
}

/** What `h` saves from a text selection, or null when nothing is selected. */
export function highlightPayload(
  selection: string,
  itemKey: string,
  view: "issue" | "article",
): { itemKey: string; quote: string; view: "issue" | "article" } | null {
  const quote = selection
    .split("\n")
    .map((line) => line.replace(/\s+/g, " ").trim())
    .filter(Boolean)
    .join("\n");
  if (!quote) return null;
  return { itemKey, view, quote: quote.slice(0, MAX_QUOTE_CHARS) };
}

/** What "let go of all" covers: every conversation the edition shows, once. */
export function letGoAllPlan(bands: readonly ReadingBand[]): {
  threadIds: string[];
  titles: string[];
} {
  const threadIds: string[] = [];
  const titles: string[] = [];
  for (const band of bands) {
    for (const item of band.items) {
      if (threadIds.includes(item.thread_id)) continue;
      threadIds.push(item.thread_id);
      titles.push(item.title);
    }
  }
  return { threadIds, titles };
}

/** "14 min", "under a minute" for nothing left to say. */
export function minutesLabel(minutes: number): string {
  return minutes <= 0 ? "done" : `${minutes} min`;
}
