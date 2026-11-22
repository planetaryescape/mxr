import type { components } from "@/api/generated";
import type { UnsubscribePurgeResponse } from "@/features/mailbox/api";
import type {
  ReadingBand,
  ReadingEdition,
  ReadingItem,
  ReadingLink,
  ReadingUnsubscribe,
} from "./api";

export const LINKS_SHOWN = 4;
export const MAX_QUOTE_CHARS = 4000;

type UnsubscribeMethod = components["schemas"]["UnsubscribeMethod"];

export type EditionEntry =
  | { kind: "item"; key: string; item: ReadingItem; band: ReadingBand["band"] }
  | {
      kind: "link";
      key: string;
      link: ReadingLink;
      parent: ReadingItem;
      band: ReadingBand["band"];
    };

export function shownLinks(links: readonly ReadingLink[], expanded: boolean) {
  if (expanded || links.length <= LINKS_SHOWN) return { shown: links, more: 0 };
  return { shown: links.slice(0, LINKS_SHOWN), more: links.length - LINKS_SHOWN };
}

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

export const READING_LIST_SEARCH = ["view", "row"] as const;

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

export function domainLabel(domain: string | null | undefined, tracked?: boolean): string {
  if (!domain) return "";
  return tracked ? `via ${domain}` : domain;
}

export function minutesLeft(words: number, progress: number, wpm: number): number {
  const left = Math.round(words * (1 - Math.min(1, Math.max(0, progress))));
  if (left <= 0) return 0;
  return Math.max(1, Math.ceil(left / Math.max(1, wpm)));
}

export function scrollProgress(scrollTop: number, scrollHeight: number, clientHeight: number) {
  const room = scrollHeight - clientHeight;
  if (room <= 0) return 1;
  return Math.min(1, Math.max(0, scrollTop / room));
}

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

export interface UnsubscribePreview {
  method: ReadingUnsubscribe;
  token: string;
  count: number;
}

function purgeMethod(method: UnsubscribeMethod | undefined): ReadingUnsubscribe {
  if (!method || method === "None") return "none";
  if ("OneClick" in method) return "one_click";
  if ("Mailto" in method) return "mailto";
  return "link";
}

export function unsubscribePreview(query: {
  status: "pending" | "error" | "success";
  data: UnsubscribePurgeResponse | undefined;
}): UnsubscribePreview | null {
  const result = query.status === "success" && query.data?.ok ? query.data.result : undefined;
  const token = result?.preview_token;
  if (!result || !token) return null;
  const method = purgeMethod(result.method);
  if (method === "none") return null;
  return { method, token, count: result.message_count };
}

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

const squash = (text: string) => text.replace(/\s+/g, " ").trim();

export function quoteOpening(quote: string, words = 8): string {
  const all = squash(quote).split(" ");
  return all.length <= words ? all.join(" ") : `${all.slice(0, words).join(" ")}…`;
}

export function highlightMatching<H extends { quote: string }>(
  selection: string,
  highlights: readonly H[],
): H | undefined {
  const wanted = squash(selection);
  if (!wanted) return undefined;
  return highlights.find((h) => squash(h.quote) === wanted);
}

export interface AnchoredMark {
  quote: string;
  paragraph?: number | null;
}

export interface ParagraphMark {
  quote: string;
  once: boolean;
}

export function marksForParagraph(
  paragraphs: readonly { text: string }[],
  marks: readonly AnchoredMark[],
  index: number,
): ParagraphMark[] {
  return marks.flatMap(({ quote, paragraph }): ParagraphMark[] => {
    if (!quote) return [];
    const anchored =
      typeof paragraph === "number" && paragraphs[paragraph]?.text.includes(quote)
        ? paragraph
        : null;
    if (anchored === null) return [{ quote, once: false }];
    return anchored === index ? [{ quote, once: true }] : [];
  });
}

export function marksForView(
  highlights: readonly {
    quote: string;
    view: string;
    origin?: string | null;
    paragraph?: number | null;
  }[],
  view: string,
): AnchoredMark[] {
  return highlights.map((h) => ({
    quote: h.quote,
    paragraph: h.view === view && isOwnHighlight(h) ? h.paragraph : null,
  }));
}

export const isOwnHighlight = (h: { origin?: string | null }) => (h.origin ?? "mxr") === "mxr";

export function letGoAllPlan(bands: readonly ReadingBand[]) {
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

export function minutesLabel(minutes: number): string {
  return minutes <= 0 ? "done" : `${minutes} min`;
}
