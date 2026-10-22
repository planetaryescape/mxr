import { Copy, FileText, Mail, X } from "lucide-react";
import { memo, type ReactNode } from "react";

import { Received } from "@/components/When";
import { Row, RowVerb, type LampVia } from "@/components/Row";
import { Button } from "@/components/ui/button";

import type { RecordAnswer, RecordAnswerList, RecordData, RecordField } from "./api";
import { longDate, matchSpan, provenanceLine, shortDate } from "./ledger";
import { SectionHeading } from "@/components/SectionHeading";
import { StatusDot } from "@/components/StatusDot";

/** An unchecked money or date field: an open dot, never red. */
export function OpenDot({ label = "unchecked" }: { label?: string }) {
  return (
    <StatusDot
      role="img"
      aria-hidden={false}
      aria-label={label}
      title="Nobody has checked this yet: press , to fix or confirm it"
      data-testid="unchecked-dot"
      className="size-2 border border-muted-foreground"
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
      className="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
    >
      {provenanceLine(field.source_label, field.checked)}
    </span>
  );
}

/**
 * One ledger line on `Row` (D129): the amount leads, as the fields you came
 * for are the amount and who it was with; the date and a PDF mark sit in the
 * trail. Whether the amount is unchecked is said in the card, per field.
 */
export const LedgerRow = memo(function LedgerRow({
  record,
  index,
  lit,
  via,
  best = false,
  onSelect,
  onOpen,
  onDocument,
}: {
  record: RecordData;
  index: number;
  lit: boolean;
  via: LampVia;
  /** The best match of a listed query. */
  best?: boolean;
  onSelect: (record: RecordData) => void;
  onOpen: (record: RecordData) => void;
  onDocument: (record: RecordData) => void;
}) {
  const what = record.title ?? record.kind_label;
  const secondary = [what, record.stage_line, record.detail_line].filter(Boolean).join(" · ");
  return (
    <Row
      data-testid="record-row"
      data-index={index}
      data-id={record.id}
      data-best={best ? "true" : undefined}
      aria-current={lit ? "true" : undefined}
      lit={lit}
      via={via}
      onClick={() => onSelect(record)}
      onDoubleClick={() => onOpen(record)}
      lead={
        <span
          data-testid="record-amount"
          className="type-meta w-24 shrink-0 text-right tabular-nums text-text-1 sm:w-28"
        >
          {record.amount?.display ?? ""}
        </span>
      }
      primary={
        <>
          {best ? <span className="sr-only">Best match: </span> : null}
          {record.issuer ?? record.kind_label}
        </>
      }
      secondary={secondary || undefined}
      trail={
        <>
          {record.reference ? (
            <span className="hidden max-w-[10rem] truncate @2xl:inline">{record.reference}</span>
          ) : null}
          {record.pdf ? <span>PDF</span> : null}
        </>
      }
      time={<span className="tabular-nums">{shortDate(record.date)}</span>}
      verbs={
        <>
          {record.thread_id ? (
            <RowVerb
              keys="o"
              label="email"
              testId="record-open-email"
              onClick={() => onOpen(record)}
            />
          ) : null}
          {record.pdf ? (
            <RowVerb
              keys="Enter"
              label="document"
              testId="record-open-document"
              onClick={() => onDocument(record)}
            />
          ) : null}
        </>
      }
    />
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
      <dt className="text-xs text-muted-foreground">{label}</dt>
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
  emailButton = false,
}: {
  record: RecordData;
  onCopy: (field: RecordField) => void;
  onOpenDocument: () => void;
  onOpenEmail: () => void;
  onIssuer: () => void;
  emailButton?: boolean;
}) {
  const fields = cardFields(record);
  return (
    <article aria-label="Record" data-testid="record-card" className="grid gap-3 px-5 py-4">
      <header>
        <p className="font-mono text-2xs uppercase tracking-[0.12em] text-muted-foreground">
          {record.kind_label}
          {record.source_count > 1 ? ` · ${record.source_count} emails` : ""}
        </p>
        <h2 className="mt-1 text-md font-semibold text-foreground">
          {record.issuer ?? record.kind_label}
        </h2>
        {record.title ? <p className="text-base text-foreground/90">{record.title}</p> : null}
      </header>
      <dl className="text-sm">
        {fields.map((field) => (
          <CardLine key={field.field} label={field.label}>
            <span className="inline-flex flex-wrap items-center gap-x-3 gap-y-0.5">
              <span className="font-mono tabular-nums text-foreground">{field.value}</span>
              {field.field === "reference" || field.field === "amount" ? (
                <button
                  type="button"
                  onClick={() => onCopy(field)}
                  aria-label={`Copy ${field.label.toLowerCase()} (${field.field === "amount" ? "Y" : "y"})`}
                  title={`Copy (${field.field === "amount" ? "Y" : "y"})`}
                  className="inline-flex h-6 items-center rounded-sm px-1 text-text-3 hover:bg-ink-lamp hover:text-text-1"
                >
                  <Copy aria-hidden className="size-3.5" />
                </button>
              ) : null}
              <Provenance field={field} />
            </span>
          </CardLine>
        ))}
      </dl>
      {record.stage_line ? (
        <p className="text-xs text-muted-foreground">{record.stage_line}</p>
      ) : null}
      {record.group ? (
        <p data-testid="record-group" className="text-xs text-foreground/90">
          Part of {record.group.kind} "{record.group.title}" ({record.group.count})
        </p>
      ) : null}
      <div className="flex flex-wrap gap-2">
        {record.pdf ? (
          <button
            type="button"
            onClick={onOpenDocument}
            data-testid="record-pdf"
            title="Open the document (Enter)"
            className="type-body inline-flex min-h-8 items-center gap-2 border border-rule px-3 text-text-1 hover:bg-ink-lamp"
          >
            <FileText aria-hidden className="size-4" />
            {record.pdf.filename}
          </button>
        ) : null}
        {/* The lit row carries `o email`; the card's own button is for the phone, where no row is lit. */}
        {emailButton ? (
          <button
            type="button"
            onClick={onOpenEmail}
            title="Open the email (o)"
            className="type-body inline-flex min-h-8 items-center gap-2 px-2 text-text-2 hover:bg-ink-lamp hover:text-text-1"
          >
            <Mail aria-hidden className="size-4" />
            Open email
          </button>
        ) : null}
      </div>
      {record.documents && record.documents.length > 1 ? (
        <ul aria-label="Documents" className="grid gap-0.5 text-xs text-muted-foreground">
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
          <SectionHeading as="h3">
            From {record.sources.length === 1 ? "1 email" : `${record.sources.length} emails`}
          </SectionHeading>
          <ul className="mt-1 grid gap-0.5 text-xs">
            {record.sources.map((source) => (
              <li key={source.message_id} className="flex min-w-0 gap-2">
                <span className="shrink-0 font-mono tabular-nums text-muted-foreground">
                  <Received value={source.date} />
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
          title="This issuer's records (p)"
          className="w-fit text-left text-xs text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
        >
          Also from {record.issuer}: {record.issuer_records} more
        </button>
      ) : null}
      <p data-testid="record-why" className="text-xs text-fg-2">
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
  onShowAll,
}: {
  answer: RecordAnswer;
  onCopy: (text: string) => void;
  onOpenDocument: (record: RecordData) => void;
  onOpenEmail: (record: RecordData) => void;
  /** List every match in place of the ledger. */
  onShowAll: () => void;
}) {
  const card = answer.answer;
  if (!card) {
    const fallback = answer.fallback;
    if (!fallback) return null;
    return (
      <section aria-label="Answer" data-testid="answer-fallback" className="mx-5 mt-3 grid gap-2">
        <p className="text-sm text-foreground/90">{fallback.note}</p>
        {!fallback.answer && !fallback.error ? (
          <p className="text-xs text-muted-foreground">Press Enter to search all mail.</p>
        ) : null}
        {fallback.answer ? (
          <div className="border-l-2 border-border pl-3">
            <p className="text-pretty text-sm italic text-foreground/90">{fallback.answer.text}</p>
            <ul className="mt-1 grid gap-0.5 text-xs text-muted-foreground">
              {fallback.answer.citations.map((citation) => (
                <li key={citation.message_id} className="truncate">
                  {shortDate(citation.date)} · {citation.subject}
                </li>
              ))}
            </ul>
          </div>
        ) : null}
        {fallback.error ? (
          <p className="text-xs text-muted-foreground">
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
      <p className="font-mono text-2xs uppercase tracking-[0.12em] text-muted-foreground">Answer</p>
      <div className="mt-1 flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <span className="text-sm text-muted-foreground">{card.label}</span>
        <span
          data-testid="answer-value"
          className="font-mono text-xl font-semibold tracking-tight text-foreground"
        >
          {card.value}
        </span>
        {card.copy ? (
          <button
            type="button"
            onClick={() => onCopy(card.copy)}
            title="Copy (y)"
            className="inline-flex h-6 items-center gap-1 px-1.5 text-xs text-text-2 hover:bg-ink-lamp hover:text-text-1"
          >
            <Copy aria-hidden className="size-3.5" /> copy
          </button>
        ) : null}
      </div>
      <p className="mt-1 text-sm text-foreground/90">
        {[record.issuer, record.title, longDate(record.span_start ?? record.date)]
          .filter(Boolean)
          .join(" · ")}
      </p>
      {record.group ? (
        <p className="text-xs text-muted-foreground">
          Part of {record.group.kind} "{record.group.title}" ({record.group.count})
        </p>
      ) : null}
      <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1">
        {card.provenance ? <Provenance field={card.provenance} /> : null}
        {record.pdf ? (
          <button
            type="button"
            onClick={() => onOpenDocument(record)}
            title="Open the document (Enter)"
            className="inline-flex items-center gap-1 text-xs text-foreground hover:underline"
          >
            <FileText aria-hidden className="size-3.5" /> {record.pdf.filename}
          </button>
        ) : null}
        <button
          type="button"
          onClick={() => onOpenEmail(record)}
          title="Open the email (o)"
          className="inline-flex items-center gap-1 text-xs text-foreground hover:underline"
        >
          <Mail aria-hidden className="size-3.5" /> email
        </button>
      </div>
      {(answer.matching ?? 0) > 1 ? (
        <button
          type="button"
          data-testid="answer-show-all"
          onClick={onShowAll}
          className="mt-2 min-h-8 text-xs text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
        >
          Show all {answer.matching} matches
        </button>
      ) : null}
    </section>
  );
}

/**
 * Over a listed query's matches: what they are, how many, what they add
 * up to and when, the issuer's page when they share one, and the way back
 * to the whole ledger.
 */
export function MatchesHeader({
  list,
  onClear,
  onIssuer,
}: {
  list: RecordAnswerList;
  onClear: () => void;
  onIssuer: (issuer: string) => void;
}) {
  const span = matchSpan(list);
  const issuer = list.issuer;
  return (
    <section aria-label="Matches" data-testid="answer-list" className="mx-5 mt-3">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h2 data-testid="answer-list-header" className="text-md font-semibold">
          {list.header}
        </h2>
        <Button
          variant="ghost"
          size="sm"
          title="Clear search (Esc)"
          className="ml-auto text-text-2"
          onClick={onClear}
        >
          <X aria-hidden className="size-3.5" /> Clear search
        </Button>
      </div>
      <p className="mt-0.5 flex flex-wrap items-center gap-x-3 text-xs text-muted-foreground">
        {span ? <span data-testid="answer-list-span">{span}</span> : null}
        {issuer ? (
          <button
            type="button"
            onClick={() => onIssuer(issuer)}
            className="min-h-8 text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
          >
            {issuer}'s page
          </button>
        ) : null}
      </p>
    </section>
  );
}
