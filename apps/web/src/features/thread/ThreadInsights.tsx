/*
 * AI overview and open commitments shown above a thread. Both come from
 * the daemon; the reader only formats them.
 */

import { Check, ChevronDown, FileText, RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export interface ThreadSummaryView {
  model?: string;
  generatedAt?: string;
  text: string;
  bullets: string[];
}

export interface ThreadCommitmentView {
  id: string;
  direction: string;
  whoOwes: string;
  what: string;
  byWhen?: string | null;
}

export function ThreadSummaryAccordion({
  summary,
  expanded,
  onExpandedChange,
}: {
  summary: ThreadSummaryView;
  expanded: boolean;
  onExpandedChange: (expanded: boolean) => void;
}) {
  return (
    <section className="mb-4 rounded-lg border border-border bg-surface">
      <button
        type="button"
        className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left"
        aria-expanded={expanded}
        onClick={() => onExpandedChange(!expanded)}
      >
        <span className="flex min-w-0 items-center gap-2">
          <FileText className="size-4 shrink-0 text-primary" />
          <span className="font-medium">AI overview</span>
          {summary.model ? (
            <span className="truncate font-mono text-2xs text-muted-foreground">
              {summary.model}
            </span>
          ) : null}
        </span>
        <ChevronDown
          className={cn(
            "size-4 shrink-0 text-muted-foreground transition-transform",
            expanded && "rotate-180",
          )}
        />
      </button>
      {expanded ? (
        <div className="break-words border-t border-border/70 px-4 py-3 text-sm leading-6 text-foreground">
          {summary.bullets.length > 0 ? (
            <ul className="space-y-1.5">
              {summary.bullets.map((item) => (
                <li key={item} className="flex gap-2">
                  <span className="mt-2 size-1.5 shrink-0 rounded-full bg-primary" />
                  <span className="min-w-0 break-words">{item}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="whitespace-pre-wrap break-words">{summary.text}</p>
          )}
        </div>
      ) : null}
    </section>
  );
}

export function ThreadSummaryLoading() {
  return (
    <section className="mb-4 rounded-lg border border-border/80 bg-muted/25 px-4 py-3 text-sm text-muted-foreground">
      <span className="flex items-center gap-2">
        <RefreshCw className="size-3.5 animate-spin" />
        Summarizing thread…
      </span>
    </section>
  );
}

export function ThreadCommitmentChips({
  commitments,
  onResolve,
  resolving,
}: {
  commitments: ThreadCommitmentView[];
  onResolve: (commitmentId: string) => void;
  resolving: boolean;
}) {
  return (
    <section
      aria-label="Open commitments"
      className="mb-4 rounded-lg border border-warning/30 bg-warning/10 px-4 py-3"
    >
      <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-foreground">
        <FileText className="size-3.5 text-warning" />
        Open commitments
      </div>
      <div className="flex flex-wrap gap-2">
        {commitments.slice(0, 4).map((commitment) => (
          <div
            key={commitment.id}
            className="flex max-w-full items-center gap-1.5 rounded-md border border-warning/40 bg-background/70 py-1 pl-2 pr-1 text-2xs"
            title={commitment.what}
          >
            <span className="font-medium">{commitment.whoOwes}</span>
            <span className="text-muted-foreground">{commitment.direction}</span>
            <span className="max-w-[280px] truncate">{commitment.what}</span>
            {commitment.byWhen ? (
              <span className="text-muted-foreground">due {shortDate(commitment.byWhen)}</span>
            ) : null}
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              className="shrink-0"
              aria-label={`Resolve commitment: ${commitment.what}`}
              disabled={resolving}
              onClick={() => onResolve(commitment.id)}
            >
              <Check className="size-3" />
            </Button>
          </div>
        ))}
      </div>
    </section>
  );
}

export function normalizeThreadSummary(payload: unknown): ThreadSummaryView | null {
  const source =
    isRecord(payload) && isRecord(payload.summary)
      ? payload.summary
      : isRecord(payload)
        ? payload
        : null;
  const text = typeof source?.text === "string" ? source.text.trim() : "";
  if (!text) return null;
  const model = typeof source?.model === "string" ? source.model : undefined;
  const generatedAt = typeof source?.generated_at === "string" ? source.generated_at : undefined;
  return { generatedAt, model, text, bullets: summaryBullets(text) };
}

export function extractThreadCommitments(payload: unknown): ThreadCommitmentView[] {
  const commitments =
    isRecord(payload) && Array.isArray(payload.commitments) ? payload.commitments : [];
  return commitments.flatMap((item) => {
    if (!isRecord(item)) return [];
    const id = typeof item.id === "string" ? item.id : "";
    const what = typeof item.what === "string" ? item.what.trim() : "";
    const whoOwes = typeof item.who_owes === "string" ? item.who_owes.trim() : "";
    const direction = typeof item.direction === "string" ? item.direction : "";
    if (!id || !what || !whoOwes || !direction) return [];
    return [
      {
        id,
        what,
        whoOwes,
        direction,
        byWhen: typeof item.by_when === "string" ? item.by_when : null,
      },
    ];
  });
}

function shortDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function summaryBullets(text: string): string[] {
  const lines = text
    .split(/\r?\n/)
    .map((line) =>
      line
        .trim()
        .replace(/^[-*•]\s+/, "")
        .replace(/^\d+[.)]\s+/, ""),
    )
    .filter((line) => !/^(summary|next steps):$/i.test(line))
    .filter(Boolean);
  if (lines.length > 1) return lines;
  return [];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
