/*
 * The AI overview (`y`) shown above a thread. It comes from the daemon; the
 * reader only formats it.
 */

import { ChevronDown, FileText, RefreshCw } from "lucide-react";

import { ModelMark } from "@/components/ModelMark";
import { StatusDot } from "@/components/StatusDot";
import { cn } from "@/lib/utils";

export interface ThreadSummaryView {
  model?: string;
  generatedAt?: string;
  text: string;
  bullets: string[];
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
    <section className="mb-4 border border-rule bg-surface">
      <button
        type="button"
        className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left"
        aria-expanded={expanded}
        onClick={() => onExpandedChange(!expanded)}
      >
        <span className="flex min-w-0 items-center gap-2">
          <FileText className="size-4 shrink-0 text-signal" />
          <span className="type-heading">AI overview</span>
          {/* The summary says which model wrote it, not where it ran. */}
          {summary.model ? <ModelMark model={summary.model} className="truncate" /> : null}
        </span>
        <ChevronDown
          className={cn(
            "size-4 shrink-0 text-text-3 transition-transform",
            expanded && "rotate-180",
          )}
        />
      </button>
      {expanded ? (
        <div className="type-body break-words border-t border-rule-soft px-4 py-3 text-text-1">
          {summary.bullets.length > 0 ? (
            <ul className="space-y-1.5">
              {summary.bullets.map((item) => (
                <li key={item} className="flex gap-2">
                  <StatusDot className="mt-2 size-1.5 bg-signal" />
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
    <section className="type-body mb-4 border border-rule-soft bg-surface px-4 py-3 text-text-3">
      <span className="flex items-center gap-2">
        <RefreshCw className="size-3.5 animate-spin" />
        Summarizing thread…
      </span>
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
