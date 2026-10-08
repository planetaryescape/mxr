/*
 * Reading over the bridge. The daemon cuts each newsletter into readable
 * items, bands and ranks them (`GetReadingEdition`), and owns the article
 * fetch and its guards; this file only moves them. Let go is the modes'
 * done (`POST /modes/reading/done`), and unsubscribe the existing purge.
 */

import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Schemas = components["schemas"];
export type ReadingEdition = Schemas["ReadingEditionData"];
export type ReadingItem = Schemas["ReadingItemData"];
export type ReadingLink = Schemas["ReadingLinkData"];
export type ReadingBand = Schemas["ReadingBandGroupData"];
export type ReadingDetail = Schemas["ReadingItemDetailData"];
export type ReadingArticle = Schemas["ReadingArticleData"];
export type ReadingFetch = Schemas["ReadingFetchData"];
export type ReadingHighlight = Schemas["ReadingHighlightData"];
export type ReadingSource = Schemas["ReadingSourceData"];
export type ReadingParagraph = Schemas["ReadingParagraphData"];
export type ReadingUnsubscribe = Schemas["ReadingUnsubscribeData"];

type Edition = Extract<Schemas["ResponseData"], { kind: "ReadingEdition" }>;
type Item = Extract<Schemas["ResponseData"], { kind: "ReadingItem" }>;
type Later = Extract<Schemas["ResponseData"], { kind: "ReadingLater" }>;
type Engagement = Extract<Schemas["ResponseData"], { kind: "ReadingEngagement" }>;
type ArticleResponse = Extract<Schemas["ResponseData"], { kind: "ReadingArticle" }>;
type HighlightResponse = Extract<Schemas["ResponseData"], { kind: "ReadingHighlight" }>;
type Highlights = Extract<Schemas["ResponseData"], { kind: "ReadingHighlights" }>;
type SourceResponse = Extract<Schemas["ResponseData"], { kind: "ReadingSource" }>;

/** Every Reading query starts with this, so one invalidation refreshes them. */
export const READING_KEY = ["reading"] as const;

const itemPath = (key: string) => `/api/v1/mail/reading/items/${encodeURIComponent(key)}`;

export async function fetchEdition(account: string | null, markVisit: boolean) {
  const query = new URLSearchParams();
  if (account) query.set("account", account);
  if (markVisit) query.set("mark_visit", "true");
  const text = query.toString();
  const answer = await apiFetch<Edition>(`/api/v1/mail/reading${text ? `?${text}` : ""}`);
  return answer.edition;
}

export async function fetchItem(key: string): Promise<ReadingDetail> {
  return (await apiFetch<Item>(itemPath(key))).item;
}

export function setLater(keys: string[], later: boolean, dryRun = false) {
  return apiFetch<Later>("/api/v1/mail/reading/later", {
    method: "POST",
    body: { item_keys: keys, later, dry_run: dryRun },
  });
}

export function recordEngagement(input: {
  itemKey: string;
  opened?: boolean;
  dwellMs?: number;
  progress?: number;
}) {
  return apiFetch<Engagement>("/api/v1/mail/reading/engagement", {
    method: "POST",
    body: {
      item_key: input.itemKey,
      opened: input.opened ?? false,
      dwell_ms: Math.max(0, Math.round(input.dwellMs ?? 0)),
      progress: Math.min(1, Math.max(0, input.progress ?? 0)),
    },
  });
}

/** Contacts the link's site: only ever called on an explicit key or button. */
export async function fetchArticle(key: string, refresh = false): Promise<ReadingFetch> {
  const answer = await apiFetch<ArticleResponse>(`${itemPath(key)}/article`, {
    method: "POST",
    body: { refresh },
  });
  return answer.fetch;
}

export async function saveHighlight(input: {
  itemKey: string;
  quote: string;
  view: "issue" | "article";
  note?: string;
}): Promise<ReadingHighlight> {
  const answer = await apiFetch<HighlightResponse>("/api/v1/mail/reading/highlights", {
    method: "POST",
    body: { item_key: input.itemKey, quote: input.quote, view: input.view, note: input.note },
  });
  return answer.highlight;
}

export function fetchHighlights(account: string | null) {
  const query = account ? `?account=${encodeURIComponent(account)}` : "";
  return apiFetch<Highlights>(`/api/v1/mail/reading/highlights${query}`);
}

export async function setSource(input: {
  accountId: string;
  senderEmail: string;
  originalLayout?: boolean;
  dismissUnsubscribeOffer?: boolean;
}): Promise<ReadingSource> {
  const answer = await apiFetch<SourceResponse>("/api/v1/mail/reading/sources", {
    method: "POST",
    body: {
      account_id: input.accountId,
      sender_email: input.senderEmail,
      original_layout: input.originalLayout,
      dismiss_unsubscribe_offer: input.dismissUnsubscribeOffer ?? false,
    },
  });
  return answer.source;
}

export function useItemQuery(key: string | undefined) {
  return useQuery({
    queryKey: [...READING_KEY, "item", key ?? ""],
    queryFn: () => fetchItem(key ?? ""),
    enabled: Boolean(key),
    staleTime: 30_000,
  });
}

export function useAccountScope(): string | null {
  return useUiPrefs((s) => s.accountScope);
}
