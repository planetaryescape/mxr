/*
 * Archive's ledger, shaped for drawing: the kind chips, records grouped
 * under the daemon's month headers, which fields still need checking, and
 * year stepping. Pure functions, so the route and its tests share them.
 */

import type { RecordData, RecordFilter, RecordKind, RecordMonth } from "./api";

export interface KindChip {
  id: "all" | "receipts" | "orders" | "trips" | "bills" | "documents";
  label: string;
  kinds: RecordKind[];
}

/** The chips over the ledger, as concrete as NN/g asks: what people call them. */
export const KIND_CHIPS: KindChip[] = [
  { id: "all", label: "All", kinds: [] },
  { id: "receipts", label: "Receipts", kinds: ["receipt", "order"] },
  { id: "orders", label: "Orders", kinds: ["order"] },
  { id: "trips", label: "Trips", kinds: ["booking", "ticket"] },
  { id: "bills", label: "Bills", kinds: ["invoice", "statement"] },
  { id: "documents", label: "Documents", kinds: ["contract", "warranty", "account"] },
];

const sameKinds = (a: readonly RecordKind[], b: readonly RecordKind[]) =>
  a.length === b.length && a.every((kind) => b.includes(kind));

/** The chip a filter's kinds pick, or none when they are a custom set. */
export function activeChip(filter: RecordFilter): KindChip["id"] | null {
  return KIND_CHIPS.find((chip) => sameKinds(chip.kinds, filter.kinds ?? []))?.id ?? null;
}

/** "2025-03" for a record's ledger date, in the viewer's zone. */
export function monthKey(record: RecordData): string | null {
  if (!record.date) return null;
  const date = new Date(record.date);
  if (Number.isNaN(date.getTime())) return null;
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}`;
}

export interface LedgerGroup {
  month: RecordMonth | null;
  records: RecordData[];
}

/**
 * This page's records under their month headers, newest first. The header's
 * count and totals are the daemon's, over every matching record, so they
 * stay right when the page holds only part of a month.
 */
export function groupByMonth(
  records: readonly RecordData[],
  months: readonly RecordMonth[],
): LedgerGroup[] {
  const byKey = new Map(months.map((month) => [month.month, month]));
  const groups: LedgerGroup[] = [];
  for (const record of records) {
    const key = monthKey(record);
    const month = key ? (byKey.get(key) ?? null) : null;
    const last = groups.at(-1);
    if (last && last.month?.month === month?.month) last.records.push(record);
    else groups.push({ month, records: [record] });
  }
  return groups;
}

/** The record's amount came from a rule nobody has confirmed. */
export function amountUnchecked(record: RecordData): boolean {
  return record.unchecked_fields?.includes("amount") ?? false;
}

/** The ledger row's date: "03 Mar". */
export function shortDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleDateString("en-GB", { day: "2-digit", month: "short" });
}

/** "3 Mar 2025". */
export function longDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}

/**
 * `[` and `]`: the year before or after the one in view, among the years
 * that have records (newest first). From no year, `[` goes to the newest.
 */
export function stepYear(
  years: readonly number[],
  current: number | null | undefined,
  delta: -1 | 1,
): number | null {
  if (years.length === 0) return current ?? null;
  const sorted = [...years].toSorted((a, b) => b - a);
  if (current == null) return delta === -1 ? sorted[0]! : null;
  const index = sorted.indexOf(current);
  if (index === -1) return sorted[0]!;
  // `[` steps back in time: to the next older year.
  const next = index + (delta === -1 ? 1 : -1);
  if (next < 0) return null;
  return sorted[Math.min(next, sorted.length - 1)]!;
}

/** What `y` copies: the reference as written. */
export function referenceCopy(record: RecordData): string | null {
  return (
    record.fields?.find((field) => field.field === "reference")?.copy ?? record.reference ?? null
  );
}

/** What `Y` copies: the amount as a plain number. */
export function amountCopy(record: RecordData): string | null {
  return record.fields?.find((field) => field.field === "amount")?.copy ?? null;
}

/** "from schema.org markup · checked", the provenance chip's words. */
export function provenanceLine(source: string, checked: boolean): string {
  return `from ${source} · ${checked ? "checked" : "unchecked"}`;
}
