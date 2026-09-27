import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

type Schemas = components["schemas"];

export type AnalyticsRange = "7d" | "30d" | "90d" | "1y";
export type StorageGroupBy = "sender" | "mimetype" | "label";
export type ResponseDirection = "they_replied" | "i_replied";

/** Row shapes come from the daemon's OpenAPI schema so they cannot drift. */
export interface StorageBucket {
  key: string;
  bytes: number;
  count: number;
}
export type LargestMessage = Schemas["LargestMessageRow"];
export type StaleThread = Schemas["StaleThreadRow"];
export type ContactAsymmetry = Schemas["ContactAsymmetryRow"];
export type ContactDecay = Schemas["ContactDecayRow"];
export type ResponseTimeSummary = Schemas["ResponseTimeSummary"];
export type WrappedSummary = Schemas["WrappedSummary"];
export type CadenceDriftRow = Schemas["CadenceDriftRowData"];
export type CadenceWatchEntry = Schemas["RelationshipWatchEntryData"];

export interface SubscriptionSummary {
  account_id?: string;
  sender_email: string;
  sender_name?: string | null;
  message_count: number;
  opened_count?: number;
  archived_unread_count?: number;
  latest_subject?: string;
  latest_snippet?: string;
  latest_message_id?: string;
  latest_thread_id?: string;
  latest_date?: string;
}

export function rangeDays(range: AnalyticsRange): number {
  switch (range) {
    case "7d":
      return 7;
    case "30d":
      return 30;
    case "90d":
      return 90;
    case "1y":
      return 365;
  }
}

export function analyticsWindow(range: AnalyticsRange) {
  const until = Math.floor(Date.now() / 1000);
  return { since_unix: until - rangeDays(range) * 24 * 60 * 60, until_unix: until };
}

export function fetchStorageBreakdown(groupBy: StorageGroupBy = "sender", limit = 20) {
  return apiFetch<{ rows: StorageBucket[] }>(
    `/api/v1/platform/analytics/storage-breakdown?group_by=${groupBy}&limit=${limit}`,
  );
}

export function fetchLargestMessages(limit = 25, sinceDays = 90) {
  return apiFetch<{ rows: LargestMessage[] }>(
    `/api/v1/platform/analytics/largest-messages?limit=${limit}&since_days=${sinceDays}`,
  );
}

export function fetchStaleThreads(
  perspective: "mine" | "theirs" = "mine",
  olderThanDays = 14,
  withinDays = 180,
) {
  return apiFetch<{ rows: StaleThread[] }>(
    `/api/v1/platform/analytics/stale-threads?perspective=${perspective}&older_than_days=${olderThanDays}&within_days=${withinDays}&limit=50`,
  );
}

export function fetchContactAsymmetry(limit = 40) {
  return apiFetch<{ rows: ContactAsymmetry[] }>(
    `/api/v1/platform/analytics/contact-asymmetry?limit=${limit}`,
  );
}

export function fetchContactDecay(limit = 40, thresholdDays = 30, maxLookbackDays = 365) {
  return apiFetch<{ rows: ContactDecay[] }>(
    `/api/v1/platform/analytics/contact-decay?threshold_days=${thresholdDays}&max_lookback_days=${maxLookbackDays}&limit=${limit}`,
  );
}

export function fetchResponseTime(sinceDays = 90, direction: ResponseDirection = "they_replied") {
  return apiFetch<{ summary: ResponseTimeSummary }>(
    `/api/v1/platform/analytics/response-time?since_days=${sinceDays}&direction=${direction}`,
  );
}

export function fetchSubscriptions(limit = 100, account?: string) {
  const query = new URLSearchParams({ limit: String(limit) });
  if (account) query.set("account", account);
  return apiFetch<{ subscriptions: SubscriptionSummary[] }>(
    `/api/v1/platform/subscriptions?${query.toString()}`,
  );
}

export function unsubscribeSubscription(messageId: string) {
  return apiFetch<unknown>("/api/v1/mail/actions/unsubscribe", {
    method: "POST",
    body: { message_id: messageId },
  });
}

export function fetchWrapped(range: AnalyticsRange) {
  const window = analyticsWindow(range);
  return apiFetch<{ summary: WrappedSummary }>(
    `/api/v1/platform/analytics/wrapped?since_unix=${window.since_unix}&until_unix=${window.until_unix}&label=wrapped`,
  );
}

function accountQuery(accountId: string | null): string {
  return accountId ? `?account=${encodeURIComponent(accountId)}` : "";
}

/** Watched contacts past their usual cadence (TUI "Cadence drift"). */
export function fetchCadenceDrift(accountId: string | null) {
  return apiFetch<{ rows: CadenceDriftRow[] }>(
    `/api/v1/platform/analytics/cadence-drift${accountQuery(accountId)}`,
  );
}

export function fetchCadenceWatchlist(accountId: string | null) {
  return apiFetch<{ entries: CadenceWatchEntry[] }>(
    `/api/v1/platform/cadence/watch${accountQuery(accountId)}`,
  );
}

export function watchCadence(input: {
  accountId: string | null;
  email: string;
  expectedDays?: number | null;
  note?: string | null;
}) {
  return apiFetch<unknown>("/api/v1/platform/cadence/watch", {
    method: "POST",
    body: {
      account_id: input.accountId ?? undefined,
      email: input.email,
      expected_days: input.expectedDays ?? undefined,
      note: input.note ?? undefined,
    },
  });
}

export function unwatchCadence(input: { accountId: string | null; email: string }) {
  return apiFetch<unknown>("/api/v1/platform/cadence/unwatch", {
    method: "POST",
    body: { account_id: input.accountId ?? undefined, email: input.email },
  });
}

export function refreshAnalyticsContacts(): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>("/api/v1/platform/analytics/refresh-contacts", {
    method: "POST",
  });
}

export function rebuildAnalytics() {
  return apiFetch<unknown>("/api/v1/platform/analytics/rebuild", { method: "POST" });
}
