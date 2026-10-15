/*
 * Archive's ledger state in the URL: the filter, the section, the question
 * asked and the selected row, so a copied link reopens the same view. The
 * route's validateSearch runs in the entry chunk, so this imports types only.
 */

import { optionalEnum, optionalString } from "@/lib/searchParams";

import type { RecordFilter, RecordKind } from "./api";

/** The kinds the bridge accepts; a facet value outside them is skipped. */
export const KNOWN_KINDS: readonly RecordKind[] = [
  "receipt",
  "order",
  "booking",
  "invoice",
  "statement",
  "ticket",
  "contract",
  "warranty",
  "account",
];

export interface ArchiveSearch {
  /** The record under the cursor. */
  record?: string;
  /** Subscriptions instead of the ledger. */
  section?: "subscriptions";
  /** The subscription under the cursor. */
  sub?: string;
  /** The question in the answer box. */
  q?: string;
  year?: number;
  issuer?: string;
  kinds?: RecordKind[];
  checked?: boolean;
  pdf?: boolean;
  group?: string;
  /** Amounts in minor units. */
  min?: number;
  max?: number;
}

const integer = (value: unknown) => (Number.isSafeInteger(value) ? (value as number) : undefined);
const boolean = (value: unknown) => (typeof value === "boolean" ? value : undefined);

export function parseArchiveSearch(search: Record<string, unknown>): ArchiveSearch {
  const kinds = Array.isArray(search.kinds)
    ? search.kinds.filter((kind): kind is RecordKind => KNOWN_KINDS.includes(kind as RecordKind))
    : [];
  return {
    record: optionalString(search.record),
    section: optionalEnum(search.section, ["subscriptions"] as const),
    sub: optionalString(search.sub),
    q: optionalString(search.q),
    year: integer(search.year),
    issuer: optionalString(search.issuer),
    kinds: kinds.length > 0 ? kinds : undefined,
    checked: boolean(search.checked),
    pdf: boolean(search.pdf),
    group: optionalString(search.group),
    min: integer(search.min),
    max: integer(search.max),
  };
}

/** The ledger filter a URL names. */
export function filterFromSearch(search: ArchiveSearch): RecordFilter {
  const filter: RecordFilter = {
    year: search.year,
    issuer: search.issuer,
    kinds: search.kinds,
    checked: search.checked,
    has_pdf: search.pdf,
    group_id: search.group,
    min_amount_minor: search.min,
    max_amount_minor: search.max,
  };
  return Object.fromEntries(
    Object.entries(filter).filter(([, value]) => value !== undefined),
  ) as RecordFilter;
}

/** A filter as URL params; every filter key is present, so it replaces the last one. */
export function searchFromFilter(filter: RecordFilter): ArchiveSearch {
  return {
    year: filter.year ?? undefined,
    issuer: filter.issuer || undefined,
    kinds: filter.kinds?.length ? filter.kinds : undefined,
    checked: filter.checked ?? undefined,
    pdf: filter.has_pdf ?? undefined,
    group: filter.group_id || undefined,
    min: filter.min_amount_minor ?? undefined,
    max: filter.max_amount_minor ?? undefined,
  };
}
