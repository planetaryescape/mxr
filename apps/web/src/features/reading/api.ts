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
export type MarginaliaStatus = Schemas["MarginaliaStatusData"];
export type MarginaliaSend = Schemas["MarginaliaSendData"];
export type MarginaliaSync = Schemas["MarginaliaSyncData"];

type Edition = Extract<Schemas["ResponseData"], { kind: "ReadingEdition" }>;
type Item = Extract<Schemas["ResponseData"], { kind: "ReadingItem" }>;
type Later = Extract<Schemas["ResponseData"], { kind: "ReadingLater" }>;
type Engagement = Extract<Schemas["ResponseData"], { kind: "ReadingEngagement" }>;
type ArticleResponse = Extract<Schemas["ResponseData"], { kind: "ReadingArticle" }>;
type HighlightResponse = Extract<Schemas["ResponseData"], { kind: "ReadingHighlight" }>;
type Highlights = Extract<Schemas["ResponseData"], { kind: "ReadingHighlights" }>;
type ChangedResponse = Extract<Schemas["ResponseData"], { kind: "ReadingHighlightChanged" }>;
export type ReadingHighlightChange = ChangedResponse["change"];
type SourceResponse = Extract<Schemas["ResponseData"], { kind: "ReadingSource" }>;
type StatusResponse = Extract<Schemas["ResponseData"], { kind: "MarginaliaStatus" }>;
type SendResponse = Extract<Schemas["ResponseData"], { kind: "MarginaliaSend" }>;
type SyncResponse = Extract<Schemas["ResponseData"], { kind: "MarginaliaSync" }>;

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
  /** The paragraph it was selected in; absent marks the first match. */
  paragraph?: number | null;
}): Promise<ReadingHighlight> {
  const answer = await apiFetch<HighlightResponse>("/api/v1/mail/reading/highlights", {
    method: "POST",
    body: {
      item_key: input.itemKey,
      quote: input.quote,
      view: input.view,
      note: input.note,
      paragraph: input.paragraph ?? undefined,
    },
  });
  return answer.highlight;
}

const highlightPath = (id: string) => `/api/v1/mail/reading/highlights/${encodeURIComponent(id)}`;

/**
 * Replaces a highlight's note. Always sends a string: blank clears it, and
 * the bridge refuses null rather than guessing. Refused for a highlight
 * made in Marginalia.
 */
export async function updateHighlightNote(
  id: string,
  note: string,
  dryRun = false,
): Promise<ReadingHighlightChange> {
  const answer = await apiFetch<ChangedResponse>(`${highlightPath(id)}/note`, {
    method: "POST",
    body: { note, dry_run: dryRun },
  });
  return answer.change;
}

export async function removeHighlightRequest(
  id: string,
  dryRun = false,
): Promise<ReadingHighlightChange> {
  const answer = await apiFetch<ChangedResponse>(`${highlightPath(id)}/remove`, {
    method: "POST",
    body: { dry_run: dryRun },
  });
  return answer.change;
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

export const MARGINALIA_KEY = [...READING_KEY, "marginalia"] as const;

export async function fetchMarginaliaStatus(): Promise<MarginaliaStatus> {
  return (await apiFetch<StatusResponse>("/api/v1/mail/reading/marginalia")).status;
}

/** Connect with a site and a token (checked by Marginalia first), or disconnect with null. */
export async function setMarginaliaConnection(
  connection: { url: string; token: string } | null,
): Promise<MarginaliaStatus> {
  const answer = await apiFetch<StatusResponse>("/api/v1/mail/reading/marginalia", {
    method: "POST",
    body: { connection },
  });
  return answer.status;
}

/** Gives the item's text to Marginalia: only on an explicit key or button. */
export async function sendToMarginalia(key: string, dryRun = false): Promise<MarginaliaSend> {
  const answer = await apiFetch<SendResponse>(`${itemPath(key)}/marginalia`, {
    method: "POST",
    body: { dry_run: dryRun },
  });
  return answer.send;
}

export async function syncMarginaliaHighlights(key?: string): Promise<MarginaliaSync> {
  const answer = await apiFetch<SyncResponse>("/api/v1/mail/reading/marginalia/highlights", {
    method: "POST",
    body: key ? { item_key: key } : {},
  });
  return answer.sync;
}

export function useMarginaliaStatus() {
  return useQuery({
    queryKey: MARGINALIA_KEY,
    queryFn: fetchMarginaliaStatus,
    staleTime: 60_000,
  });
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
