import { apiFetch } from "@/api/client";

export type AnalyticsRange = "7d" | "30d" | "90d" | "1y";
export type StorageGroupBy = "sender" | "mimetype" | "label";
export type ResponseDirection = "they_replied" | "i_replied";

export interface StorageBucket {
  label?: string;
  key?: string;
  value?: string;
  bytes?: number;
  total_bytes?: number;
  count?: number;
}
export interface LargestMessage {
  message_id?: string;
  thread_id?: string;
  sender?: string;
  subject?: string;
  size_bytes?: number;
  date?: string;
}
export interface StaleThread {
  thread_id?: string;
  subject?: string;
  counterparty?: string;
  age_days?: number;
  message_count?: number;
  latest_at?: string;
}
export interface ContactRow {
  email?: string;
  display_name?: string;
  inbound?: number;
  outbound?: number;
  total_inbound?: number;
  total_outbound?: number;
  days_since_last_seen?: number;
  score?: number;
}
export interface ResponseTimeSummary {
  p50_minutes?: number;
  p90_minutes?: number;
  p95_minutes?: number;
  count?: number;
  buckets?: Array<{ label: string; value: number }>;
}
export interface WrappedSummary {
  volume?: { inbound_count?: number; outbound_count?: number; thread_count?: number };
  top_contacts?: {
    most_emailed_to_me?: ContactRow[];
    most_emailed_by_me?: ContactRow[];
    most_asymmetric?: ContactRow[];
  };
  superlatives?: {
    longest_thread?: { subject?: string; message_count?: number } | null;
    most_ghosted?: { email?: string; inbound_count?: number; outbound_count?: number } | null;
  };
  [key: string]: unknown;
}

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

function rangeToDays(range: AnalyticsRange): number {
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
  return { since_unix: until - rangeToDays(range) * 24 * 60 * 60, until_unix: until };
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

export function fetchStaleThreads(perspective = "mine", olderThanDays = 14, withinDays = 180) {
  return apiFetch<{ rows: StaleThread[] }>(
    `/api/v1/platform/analytics/stale-threads?perspective=${perspective}&older_than_days=${olderThanDays}&within_days=${withinDays}&limit=50`,
  );
}

export function fetchContactAsymmetry(limit = 40) {
  return apiFetch<{ rows: ContactRow[] }>(
    `/api/v1/platform/analytics/contact-asymmetry?limit=${limit}`,
  );
}

export function fetchContactDecay(limit = 40, thresholdDays = 30, maxLookbackDays = 365) {
  return apiFetch<{ rows: ContactRow[] }>(
    `/api/v1/platform/analytics/contact-decay?threshold_days=${thresholdDays}&max_lookback_days=${maxLookbackDays}&limit=${limit}`,
  );
}

export function fetchResponseTime(sinceDays = 90, direction: ResponseDirection = "they_replied") {
  return apiFetch<{ summary: ResponseTimeSummary }>(
    `/api/v1/platform/analytics/response-time?since_days=${sinceDays}&direction=${direction}`,
  );
}

export function fetchSubscriptions(limit = 100) {
  return apiFetch<{ subscriptions: SubscriptionSummary[] }>(
    `/api/v1/platform/subscriptions?limit=${limit}`,
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

export function refreshAnalyticsContacts(): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>("/api/v1/platform/analytics/refresh-contacts", {
    method: "POST",
  });
}

export function rebuildAnalytics() {
  return apiFetch<unknown>("/api/v1/platform/analytics/rebuild", { method: "POST" });
}
