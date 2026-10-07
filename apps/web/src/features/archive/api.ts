/*
 * Archive over the bridge. The daemon files the records, builds the
 * ledger's months, totals and facets, ranks answers and writes the CSV;
 * this file only moves them.
 */

import { keepPreviousData, useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Schemas = components["schemas"];
export type RecordData = Schemas["RecordData"];
export type RecordLedger = Schemas["RecordLedgerData"];
export type RecordAnswer = Schemas["RecordAnswerData"];
export type RecordChange = Schemas["RecordChangeData"];
export type RecordExport = Schemas["RecordExportData"];
export type RecordField = Schemas["RecordFieldData"];
export type RecordKind = Schemas["RecordKindData"];
export type RecordFilter = Schemas["RecordFilterData"];
export type RecordEdit = Schemas["RecordEditData"];
export type RecordMonth = Schemas["RecordMonthData"];

type Ledger = Extract<Schemas["ResponseData"], { kind: "RecordLedger" }>;
type One = Extract<Schemas["ResponseData"], { kind: "Record" }>;
type Answer = Extract<Schemas["ResponseData"], { kind: "RecordAnswer" }>;
type Change = Extract<Schemas["ResponseData"], { kind: "RecordChange" }>;
type Export = Extract<Schemas["ResponseData"], { kind: "RecordExport" }>;

/** Every Archive query starts with this, so one invalidation refreshes them. */
export const RECORDS_KEY = ["records"] as const;

/** The ledger's filter as the bridge's query string. */
export function ledgerQuery(
  account: string | null,
  filter: RecordFilter,
  page: { limit?: number; offset?: number } = {},
): string {
  const query = new URLSearchParams();
  if (account) query.set("account", account);
  if (filter.kinds?.length) query.set("kind", filter.kinds.join(","));
  if (filter.issuer) query.set("issuer", filter.issuer);
  if (filter.year != null) query.set("year", String(filter.year));
  if (filter.min_amount_minor != null)
    query.set("min_amount_minor", String(filter.min_amount_minor));
  if (filter.max_amount_minor != null)
    query.set("max_amount_minor", String(filter.max_amount_minor));
  if (filter.has_pdf != null) query.set("has_pdf", String(filter.has_pdf));
  if (filter.checked != null) query.set("checked", String(filter.checked));
  if (filter.group_id) query.set("group", filter.group_id);
  if (page.limit != null) query.set("limit", String(page.limit));
  if (page.offset != null) query.set("offset", String(page.offset));
  const text = query.toString();
  return text ? `?${text}` : "";
}

export async function fetchLedger(
  account: string | null,
  filter: RecordFilter,
  limit?: number,
): Promise<RecordLedger> {
  const answer = await apiFetch<Ledger>(
    `/api/v1/mail/records${ledgerQuery(account, filter, { limit })}`,
  );
  return answer.ledger;
}

/** The ledger for a filter: `limit` rows, newest first (the daemon's default is 200). */
export function useLedger(filter: RecordFilter, limit?: number, enabled = true) {
  const account = useUiPrefs((s) => s.accountScope);
  return useQuery({
    queryKey: [...RECORDS_KEY, "ledger", account ?? "all", filter, limit ?? null],
    queryFn: () => fetchLedger(account, filter, limit),
    enabled,
    placeholderData: keepPreviousData,
    staleTime: 15_000,
    // Records file in the background after sync and on the first run.
    refetchInterval: 30_000,
  });
}

export async function fetchRecord(id: string): Promise<RecordData> {
  const answer = await apiFetch<One>(`/api/v1/mail/records/${encodeURIComponent(id)}`);
  return answer.record;
}

export function useRecord(id: string | null) {
  return useQuery({
    queryKey: [...RECORDS_KEY, "record", id],
    queryFn: () => fetchRecord(id ?? ""),
    enabled: Boolean(id),
    placeholderData: keepPreviousData,
  });
}

export async function fetchAnswer(account: string | null, query: string): Promise<RecordAnswer> {
  const params = new URLSearchParams({ q: query });
  if (account) params.set("account", account);
  const answer = await apiFetch<Answer>(`/api/v1/mail/records/answer?${params.toString()}`);
  return answer.answer;
}

export function useAnswer(query: string) {
  const account = useUiPrefs((s) => s.accountScope);
  const text = query.trim();
  return useQuery({
    queryKey: [...RECORDS_KEY, "answer", account ?? "all", text],
    queryFn: () => fetchAnswer(account, text),
    enabled: text.length > 0,
    placeholderData: keepPreviousData,
    staleTime: 30_000,
  });
}

async function change(path: string, body: unknown): Promise<RecordChange> {
  const answer = await apiFetch<Change>(path, { method: "POST", body });
  return answer.change;
}

export function setRecordField(
  id: string,
  edit: RecordEdit,
  options: { dryRun?: boolean; applyToSender?: boolean } = {},
) {
  return change(`/api/v1/mail/records/${encodeURIComponent(id)}/field`, {
    edit,
    apply_to_sender: options.applyToSender ?? false,
    dry_run: options.dryRun ?? false,
  });
}

export function dismissRecords(ids: string[], restore = false, dryRun = false) {
  return change("/api/v1/mail/records/dismiss", { record_ids: ids, restore, dry_run: dryRun });
}

export function fileRecord(messageId: string, dryRun: boolean, kind?: RecordKind) {
  return change("/api/v1/mail/records/file", { message_id: messageId, kind, dry_run: dryRun });
}

export async function exportRecords(
  account: string | null,
  filter: RecordFilter,
  dryRun: boolean,
): Promise<RecordExport> {
  const answer = await apiFetch<Export>("/api/v1/mail/records/export", {
    method: "POST",
    body: { account_id: account ?? undefined, filter, dry_run: dryRun },
  });
  return answer.export;
}
