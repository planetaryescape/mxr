import type {
  RecordAnswer,
  RecordAnswerList,
  RecordData,
  RecordFilter,
  RecordKind,
  RecordMonth,
} from "./api";

export interface KindChip {
  id: "all" | "receipts" | "orders" | "trips" | "bills" | "documents";
  label: string;
  kinds: RecordKind[];
}

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

export function activeChip(filter: RecordFilter): KindChip["id"] | null {
  return KIND_CHIPS.find((chip) => sameKinds(chip.kinds, filter.kinds ?? []))?.id ?? null;
}

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

export function amountUnchecked(record: RecordData): boolean {
  return record.unchecked_fields?.includes("amount") ?? false;
}

export function shortDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleDateString("en-GB", { day: "2-digit", month: "short" });
}

export function longDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}

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
  const next = index + (delta === -1 ? 1 : -1);
  if (next < 0) return null;
  return sorted[Math.min(next, sorted.length - 1)]!;
}

export function referenceCopy(record: RecordData): string | null {
  return (
    record.fields?.find((field) => field.field === "reference")?.copy ?? record.reference ?? null
  );
}

export function amountCopy(record: RecordData): string | null {
  return record.fields?.find((field) => field.field === "amount")?.copy ?? null;
}

export function provenanceLine(source: string, checked: boolean): string {
  return `from ${source} · ${checked ? "checked" : "unchecked"}`;
}

export function answerList(answer: RecordAnswer | undefined): RecordAnswerList | null {
  return answer?.mode === "list" ? (answer.list ?? null) : null;
}

export function matchSpan(list: RecordAnswerList): string {
  const first = longDate(list.first);
  const last = longDate(list.last);
  if (!first || !last) return first || last;
  return first === last ? first : `${first} to ${last}`;
}
