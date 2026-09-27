/*
 * Pieces every analytics dashboard shares: drill-down navigation (the TUI
 * drills every row into a search), the ranked bar list, and duration text.
 */

import { useNavigate } from "@tanstack/react-router";
import type { ReactNode } from "react";

import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "@/lib/utils";

/** Open a search for `q`, the way the TUI's analytics drill-down does. */
export function useDrillToSearch(): (q: string) => void {
  const navigate = useNavigate();
  return (q: string) => void navigate({ to: "/search", search: { q } });
}

/** Open a conversation. All Mail holds every thread, whatever its labels. */
export function useOpenThread(): (threadId: string) => void {
  const navigate = useNavigate();
  return (threadId: string) =>
    void navigate({ to: "/m/$mailbox/$threadId", params: { mailbox: "archive", threadId } });
}

/** Quote a search operator value when it has spaces. */
export function searchValue(value: string): string {
  return /\s/.test(value) ? `"${value.replace(/"/g, "")}"` : value;
}

export interface BarRow {
  id: string;
  label: string;
  value: number;
  /** Formatted value shown at the row end. */
  display: string;
  /** Mono meta after the label (count, share). */
  meta?: string;
  onSelect?: () => void;
}

/**
 * A ranked horizontal bar list. Chosen over a column chart because the
 * rows are long categorical labels (senders, MIME types, buckets) that a
 * column chart can only show on hover; here every label is readable and
 * each row is a keyboard-reachable drill-down.
 */
export function BarList({
  rows,
  label,
  className,
}: {
  rows: BarRow[];
  label: string;
  className?: string;
}) {
  const max = Math.max(1, ...rows.map((row) => row.value));
  return (
    <ul aria-label={label} className={cn(className)}>
      {rows.map((row) => {
        const width = `${Math.max(row.value > 0 ? 1.5 : 0, (row.value / max) * 100)}%`;
        const body = (
          <>
            <div className="flex items-baseline gap-3">
              <span className="min-w-0 flex-1 truncate text-[13px]">{row.label}</span>
              {row.meta ? (
                <span className="shrink-0 font-mono text-2xs text-muted-foreground">
                  {row.meta}
                </span>
              ) : null}
              <span className="w-20 shrink-0 text-right font-mono text-2xs tabular-nums">
                {row.display}
              </span>
            </div>
            <div className="mt-1 h-1.5 rounded-full bg-muted/60" aria-hidden="true">
              <div className="h-1.5 rounded-full bg-chart-1" style={{ width }} />
            </div>
          </>
        );
        return (
          <li
            key={row.id}
            className="border-b border-border/60"
            title={`${row.label}: ${row.display}`}
          >
            {row.onSelect ? (
              <button
                type="button"
                onClick={row.onSelect}
                className="block w-full px-2 py-2 text-left outline-none hover:bg-muted/40 focus-visible:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring"
              >
                {body}
              </button>
            ) : (
              <div className="px-2 py-2">{body}</div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** Big-number figure for a dashboard headline (one per section, not a grid). */
export function Figure({
  label,
  value,
  caption,
}: {
  label: string;
  value: ReactNode;
  caption?: ReactNode;
}) {
  return (
    <div className="min-w-0">
      <div className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
        {label}
      </div>
      <div className="mt-1 text-2xl font-semibold tabular-nums tracking-tight">{value}</div>
      {caption ? <div className="mt-0.5 text-[12.5px] text-muted-foreground">{caption}</div> : null}
    </div>
  );
}

/** "45s", "12m", "3.5h", "2.1d" for reply latencies. */
export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds)) return "n/a";
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
  if (seconds < 86_400) return `${(seconds / 3600).toFixed(1).replace(/\.0$/, "")}h`;
  return `${(seconds / 86_400).toFixed(1).replace(/\.0$/, "")}d`;
}

export function formatDays(days: number): string {
  const rounded = Math.round(days * 10) / 10;
  return `${rounded.toLocaleString()}d`;
}

/** A small single-choice segmented control for dashboard options. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { id: T; label: string }[];
  onChange: (value: T) => void;
  label: string;
}) {
  return (
    <ToggleGroup
      type="single"
      value={value}
      onValueChange={(next) => {
        if (next) onChange(next as T);
      }}
      aria-label={label}
    >
      {options.map((option) => (
        <ToggleGroupItem key={option.id} value={option.id} size="sm" className="px-2.5">
          {option.label}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}
