/*
 * Building blocks shared by the non-mail pages (analytics, rules, accounts,
 * diagnostics…): the loading skeleton, the error-with-retry state, ruled
 * rows and a small inline notice. Pages compose these inside `Page` so every
 * page loads, fails and lists things the same way.
 */

import { AlertTriangle, RefreshCw } from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** Ruled placeholder rows, shown while a query has no data yet. */
export function PageSkeleton({ rows = 6, label = "Loading" }: { rows?: number; label?: string }) {
  return (
    <div aria-busy="true" aria-label={label} role="status">
      {Array.from({ length: rows }, (_, index) => (
        <div key={index} className="grid gap-1.5 border-b border-border/60 py-3">
          <div
            className="h-3 w-1/3 animate-pulse rounded bg-muted"
            style={{ animationDelay: `${index * 40}ms` }}
          />
          <div
            className="h-2.5 w-2/3 animate-pulse rounded bg-muted/70"
            style={{ animationDelay: `${index * 40}ms` }}
          />
        </div>
      ))}
    </div>
  );
}

/** A failed query: the daemon's own message and a retry. */
export function PageError({
  title,
  error,
  onRetry,
}: {
  title: string;
  error: Error | null | undefined;
  onRetry?: () => void;
}) {
  return (
    <div
      role="alert"
      className="flex flex-col items-center justify-center gap-2 px-6 py-14 text-center"
    >
      <AlertTriangle className="size-5 text-destructive" />
      <h2 className="text-[15px] font-semibold">{title}</h2>
      <p className="max-w-md break-words font-mono text-2xs text-muted-foreground">
        {error?.message || "The daemon did not answer."}
      </p>
      {onRetry ? (
        <Button variant="outline" size="sm" className="mt-2" onClick={onRetry}>
          <RefreshCw className="size-3" />
          Retry
        </Button>
      ) : null}
    </div>
  );
}

/** Nothing to show, and what to do about it. */
export function PageEmpty({
  icon,
  title,
  body,
  action,
}: {
  icon?: ReactNode;
  title: string;
  body?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-14 text-center">
      {icon ? <div className="mb-1 text-muted-foreground">{icon}</div> : null}
      <h2 className="text-[15px] font-semibold">{title}</h2>
      {body ? <p className="max-w-sm text-[13px] text-muted-foreground">{body}</p> : null}
      {action ? <div className="mt-2">{action}</div> : null}
    </div>
  );
}

/** A list of ruled rows (DESIGN.md: rules, not cards). */
export function RuledList({
  children,
  label,
  className,
}: {
  children: ReactNode;
  label?: string;
  className?: string;
}) {
  return (
    <ul aria-label={label} className={cn(className)}>
      {children}
    </ul>
  );
}

/**
 * One ruled row: a title line, a mono meta line, and trailing actions.
 * `onOpen` makes the text a button so the row is keyboard reachable.
 */
export function RuledRow({
  title,
  meta,
  aside,
  actions,
  onOpen,
  openLabel,
  current,
  className,
  children,
}: {
  title: ReactNode;
  meta?: ReactNode;
  /** Right-aligned value (size, count, date). */
  aside?: ReactNode;
  actions?: ReactNode;
  onOpen?: () => void;
  openLabel?: string;
  /** Keyboard cursor on this row. */
  current?: boolean;
  className?: string;
  children?: ReactNode;
}) {
  const text = (
    <>
      <div className="truncate text-[13px] font-medium text-foreground">{title}</div>
      {meta ? (
        <div className="mt-0.5 truncate font-mono text-2xs text-muted-foreground">{meta}</div>
      ) : null}
    </>
  );
  return (
    <li
      aria-current={current ? "true" : undefined}
      className={cn(
        "group flex items-center gap-3 border-b border-border/60 px-2 py-2.5",
        current ? "bg-primary-muted" : "hover:bg-muted/40",
        className,
      )}
    >
      {onOpen ? (
        <button
          type="button"
          onClick={onOpen}
          aria-label={openLabel}
          className="min-w-0 flex-1 rounded-sm text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {text}
        </button>
      ) : (
        <div className="min-w-0 flex-1">{text}</div>
      )}
      {children}
      {aside ? (
        <div className="shrink-0 font-mono text-2xs tabular-nums text-muted-foreground">
          {aside}
        </div>
      ) : null}
      {actions ? <div className="flex shrink-0 items-center gap-1.5">{actions}</div> : null}
    </li>
  );
}

/** Label/value pairs in two ruled columns (status panels, account details). */
export function FactList({
  facts,
  columns = 2,
}: {
  facts: Array<[string, ReactNode]>;
  columns?: 1 | 2;
}) {
  return (
    <dl className={cn("grid gap-x-8", columns === 2 && "sm:grid-cols-2")}>
      {facts.map(([label, value]) => (
        <div
          key={label}
          className="flex items-baseline justify-between gap-4 border-b border-border/60 py-1.5"
        >
          <dt className="font-mono text-2xs uppercase tracking-[0.08em] text-muted-foreground">
            {label}
          </dt>
          <dd className="min-w-0 truncate text-right text-[13px] tabular-nums">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

/** Explanatory prose under a section title; not an alert box. */
export function PageNote({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <p className={cn("mb-3 max-w-[65ch] text-[12.5px] text-muted-foreground", className)}>
      {children}
    </p>
  );
}
