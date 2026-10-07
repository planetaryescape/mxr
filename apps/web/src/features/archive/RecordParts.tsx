import { Copy, FileText, Mail } from "lucide-react";
import { memo, type ReactNode } from "react";

import { KeyChip } from "@/components/KeyChip";
import { cn } from "@/lib/utils";

import type { RecordAnswer, RecordData, RecordField } from "./api";
import { amountUnchecked, longDate, provenanceLine, shortDate } from "./ledger";

/** An unchecked money or date field: an open dot, never red. */
export function OpenDot({ label = "unchecked" }: { label?: string }) {
  return (
    <span
      role="img"
      aria-label={label}
      title="Nobody has checked this yet: press , to fix or confirm it"
      data-testid="unchecked-dot"
      className="inline-block size-2 shrink-0 rounded-full border border-muted-foreground"
    />
  );
}

/** "from schema.org markup · checked", with the words it was read from on hover. */
export function Provenance({ field }: { field: RecordField }) {
  const evidence = field.evidence && field.evidence !== field.value ? field.evidence : null;
  return (
    <span
      data-testid="record-provenance"
      title={evidence ? `Read from: "${evidence}"` : undefined}
      className="inline-flex items-center gap-1.5 text-[12px] text-muted-foreground"
    >
      {field.checked ? null : <OpenDot />}
      {provenanceLine(field.source_label, field.checked)}
    </span>
  );
}

/** One ledger line: the fields you came for, not sender and subject. */
export const LedgerRow = memo(function LedgerRow({
  record,
  index,
  focused,
  onSelect,
  onOpen,
}: {
  record: RecordData;
  index: number;
  focused: boolean;
  onSelect: (record: RecordData) => void;
  onOpen: (record: RecordData) => void;
}) {
  const what = record.title ?? record.kind_label;
  return (
    <li
      data-index={index}
      data-testid="record-row"
      data-id={record.id}
      aria-current={focused ? "true" : undefined}
      className={cn(
        "group mx-2 cursor-default rounded-md px-3 py-2 text-[13px]",
        focused ? "bg-accent" : "hover:bg-accent/60",
      )}
      onClick={() => onSelect(record)}
      onDoubleClick={() => onOpen(record)}
    >
      <div className="grid grid-cols-[3.25rem_minmax(0,1fr)_auto] items-baseline gap-x-3 @2xl:grid-cols-[3.25rem_minmax(0,9rem)_minmax(0,1fr)_7.5rem_6.5rem_2.5rem]">
        <span className="font-mono text-[12px] tabular-nums text-muted-foreground">
          {shortDate(record.date)}
        </span>
        <span className="min-w-0 truncate font-medium text-foreground @2xl:order-none">
          {record.issuer ?? record.kind_label}
          <span className="font-normal text-muted-foreground @2xl:hidden"> · {what}</span>
        </span>
        <span className="hidden min-w-0 truncate text-foreground/90 @2xl:block">{what}</span>
        <span
          data-testid="record-amount"
          className="inline-flex items-center justify-end gap-1.5 font-mono tabular-nums text-foreground"
        >
          {record.amount && amountUnchecked(record) ? <OpenDot label="amount unchecked" /> : null}
          {record.amount?.display ?? ""}
        </span>
        <span className="hidden min-w-0 truncate font-mono text-[12px] text-muted-foreground @2xl:block">
          {record.reference ?? ""}
        </span>
        <span className="hidden text-right font-mono text-[11px] text-muted-foreground @2xl:block">
          {record.pdf ? "PDF" : "-"}
        </span>
      </div>
      {record.stage_line || record.detail_line ? (
        <p className="ml-[4rem] mt-0.5 truncate text-[12px] text-muted-foreground">
          {[record.stage_line, record.detail_line].filter(Boolean).join(" · ")}
        </p>
      ) : null}
    </li>
  );
});

/** The card's field order: what you came for first, then the dates. */
const CARD_ORDER = [
  "reference",
  "amount",
  "issued_at",
  "span_start",
  "span_end",
  "place",
  "delivered_at",
  "return_by",
  "warranty_until",
  "valid_until",
  "kind",
];

export function cardFields(record: RecordData): RecordField[] {
  const rank = (field: RecordField) => {
    const at = CARD_ORDER.indexOf(field.field);
    return at === -1 ? CARD_ORDER.length : at;
  };
  return (record.fields ?? [])
    .filter((field) => field.field !== "issuer" && field.field !== "title")
    .toSorted((a, b) => rank(a) - rank(b));
}

function CardLine({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[7rem_minmax(0,1fr)] items-baseline gap-x-3 py-1">
      <dt className="text-[12px] text-muted-foreground">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </div>
  );
}

/**
 * The record card: every field with where it came from, its documents and
 * the emails it was built from. The email is evidence, one key away.
 */
export function RecordCard({
  record,
  onCopy,
  onOpenDocument,
  onOpenEmail,
  onIssuer,
}: {
  record: RecordData;
  onCopy: (field: RecordField) => void;
  onOpenDocument: () => void;
  onOpenEmail: () => void;
  onIssuer: () => void;
}) {
  const fields = cardFields(record);
  return (
    <article aria-label="Record" data-testid="record-card" className="grid gap-3 px-5 py-4">
      <header>
        <p className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
          {record.kind_label}
          {record.source_count > 1 ? ` · ${record.source_count} emails` : ""}
        </p>
        <h2 className="mt-1 text-[16px] font-semibold text-foreground">
          {record.issuer ?? record.kind_label}
        </h2>
        {record.title ? <p className="text-[14px] text-foreground/90">{record.title}</p> : null}
      </header>
      <dl className="text-[13px]">
        {fields.map((field) => (
          <CardLine key={field.field} label={field.label}>
            <span className="inline-flex flex-wrap items-center gap-x-3 gap-y-0.5">
              <span className="font-mono tabular-nums text-foreground">{field.value}</span>
              {field.field === "reference" || field.field === "amount" ? (
                <button
                  type="button"
                  onClick={() => onCopy(field)}
                  aria-label={`Copy ${field.label.toLowerCase()} (${field.field === "amount" ? "Y" : "y"})`}
                  className="inline-flex items-center gap-1 rounded px-1 text-[12px] text-muted-foreground hover:bg-accent hover:text-foreground"
                >
                  <Copy aria-hidden className="size-3" />
                  <KeyChip className="h-4 px-1">{field.field === "amount" ? "Y" : "y"}</KeyChip>
                </button>
              ) : null}
              <Provenance field={field} />
            </span>
          </CardLine>
        ))}
      </dl>
      {record.stage_line ? (
        <p className="text-[12.5px] text-muted-foreground">{record.stage_line}</p>
      ) : null}
      {record.group ? (
        <p data-testid="record-group" className="text-[12.5px] text-foreground/90">
          Part of {record.group.kind} "{record.group.title}" ({record.group.count})
        </p>
      ) : null}
      <div className="flex flex-wrap gap-2">
        {record.pdf ? (
          <button
            type="button"
            onClick={onOpenDocument}
            data-testid="record-pdf"
            className="inline-flex min-h-10 items-center gap-2 rounded-md border border-border bg-background px-3 text-[13px] font-medium hover:bg-accent"
          >
            <FileText aria-hidden className="size-4" />
            {record.pdf.filename}
            <KeyChip className="h-4 px-1">↵</KeyChip>
          </button>
        ) : null}
        <button
          type="button"
          onClick={onOpenEmail}
          className="inline-flex min-h-10 items-center gap-2 rounded-md border border-border bg-background px-3 text-[13px] hover:bg-accent"
        >
          <Mail aria-hidden className="size-4" />
          The email
          <KeyChip className="h-4 px-1">o</KeyChip>
        </button>
      </div>
      {record.documents && record.documents.length > 1 ? (
        <ul aria-label="Documents" className="grid gap-0.5 text-[12.5px] text-muted-foreground">
          {record.documents.map((document) => (
            <li key={document.attachment_id} className="truncate">
              {document.filename} · {Math.max(1, Math.round(document.size_bytes / 1024))} KB
              {document.on_disk ? "" : " · downloads when opened"}
            </li>
          ))}
        </ul>
      ) : null}
      {record.sources && record.sources.length > 0 ? (
        <section aria-label="From these emails">
          <h3 className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
            From {record.sources.length === 1 ? "1 email" : `${record.sources.length} emails`}
          </h3>
          <ul className="mt-1 grid gap-0.5 text-[12.5px]">
            {record.sources.map((source) => (
              <li key={source.message_id} className="flex min-w-0 gap-2">
                <span className="shrink-0 font-mono tabular-nums text-muted-foreground">
                  {shortDate(source.date)}
                </span>
                <span className="shrink-0 text-muted-foreground">{source.stage}</span>
                <span className="min-w-0 truncate">{source.subject}</span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {record.issuer && record.issuer_records ? (
        <button
          type="button"
          onClick={onIssuer}
          className="w-fit text-left text-[12.5px] text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
        >
          Also from {record.issuer}: {record.issuer_records} more{" "}
          <KeyChip className="h-4 px-1">p</KeyChip>
        </button>
      ) : null}
      <p data-testid="record-why" className="text-[12.5px] text-muted-foreground">
        {record.why}
      </p>
    </article>
  );
}

/**
 * The answer card: the field the query asked for, where it came from, and
 * the document and email one key away. When no record matches, the
 * daemon's fallback says so before showing what it found in all mail.
 */
export function AnswerCard({
  answer,
  onCopy,
  onOpenDocument,
  onOpenEmail,
  onSelect,
}: {
  answer: RecordAnswer;
  onCopy: (text: string) => void;
  onOpenDocument: (record: RecordData) => void;
  onOpenEmail: (record: RecordData) => void;
  onSelect: (record: RecordData) => void;
}) {
  const card = answer.answer;
  if (!card) {
    const fallback = answer.fallback;
    if (!fallback) return null;
    return (
      <section aria-label="Answer" data-testid="answer-fallback" className="mx-5 mt-3 grid gap-2">
        <p className="text-[13px] text-foreground/90">{fallback.note}</p>
        {!fallback.answer && !fallback.error ? (
          <p className="text-[12.5px] text-muted-foreground">
            Press <KeyChip>Enter</KeyChip> to search all mail.
          </p>
        ) : null}
        {fallback.answer ? (
          <div className="border-l-2 border-border pl-3">
            <p className="text-pretty text-[13px] italic text-foreground/90">
              {fallback.answer.text}
            </p>
            <ul className="mt-1 grid gap-0.5 text-[12.5px] text-muted-foreground">
              {fallback.answer.citations.map((citation) => (
                <li key={citation.message_id} className="truncate">
                  {shortDate(citation.date)} · {citation.subject}
                </li>
              ))}
            </ul>
          </div>
        ) : null}
        {fallback.error ? (
          <p className="text-[12.5px] text-muted-foreground">
            Searching all mail failed: {fallback.error}
          </p>
        ) : null}
      </section>
    );
  }
  const record = card.record;
  return (
    <section
      aria-label="Answer"
      data-testid="answer-card"
      className="mx-5 mt-3 border-l-2 border-primary bg-surface px-4 py-3"
    >
      <p className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
        Answer
      </p>
      <div className="mt-1 flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <span className="text-[13px] text-muted-foreground">{card.label}</span>
        <span
          data-testid="answer-value"
          className="font-mono text-[20px] font-semibold tracking-tight text-foreground"
        >
          {card.value}
        </span>
        {card.copy ? (
          <button
            type="button"
            onClick={() => onCopy(card.copy)}
            className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[12px] text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            <Copy aria-hidden className="size-3" /> copy <KeyChip className="h-4 px-1">y</KeyChip>
          </button>
        ) : null}
      </div>
      <p className="mt-1 text-[13px] text-foreground/90">
        {[record.issuer, record.title, longDate(record.span_start ?? record.date)]
          .filter(Boolean)
          .join(" · ")}
      </p>
      {record.group ? (
        <p className="text-[12.5px] text-muted-foreground">
          Part of {record.group.kind} "{record.group.title}" ({record.group.count})
        </p>
      ) : null}
      <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1">
        {card.provenance ? <Provenance field={card.provenance} /> : null}
        {record.pdf ? (
          <button
            type="button"
            onClick={() => onOpenDocument(record)}
            className="inline-flex items-center gap-1 text-[12.5px] text-foreground hover:underline"
          >
            <KeyChip className="h-4 px-1">↵</KeyChip> {record.pdf.filename}
          </button>
        ) : null}
        <button
          type="button"
          onClick={() => onOpenEmail(record)}
          className="inline-flex items-center gap-1 text-[12.5px] text-foreground hover:underline"
        >
          <KeyChip className="h-4 px-1">o</KeyChip> email
        </button>
      </div>
      {answer.also && answer.also.length > 0 ? (
        <p data-testid="answer-also" className="mt-2 text-[12.5px] text-muted-foreground">
          Also matching:{" "}
          {answer.also.map((other, at) => (
            <span key={other.id}>
              {at > 0 ? ", " : ""}
              <button
                type="button"
                onClick={() => onSelect(other)}
                className="text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
              >
                {other.title ?? other.issuer ?? other.kind_label}
                {other.reference ? ` (ref ${other.reference})` : ""}
              </button>
            </span>
          ))}
        </p>
      ) : null}
    </section>
  );
}
