/*
 * Building blocks shared by every page: the empty, loading and error states,
 * ruled rows and a small inline notice. Pages compose these so every page
 * loads, fails and says it has nothing the same way.
 */

import { Link } from "@tanstack/react-router";
import { useEffect, useState, type ReactNode } from "react";

import { MODE_GLYPHS, type ModeGlyphId } from "@/components/ModeMark";
import { ModeFrame } from "@/components/ModeFrame";
import { AvatarSkeleton } from "@/components/Avatar";
import { explainError, type ErrorFix } from "@/components/RequestError";
import { Row } from "@/components/Row";
import { Button } from "@/components/ui/button";
import { copyCommand, START_COMMAND, UPDATE_COMMAND } from "@/lib/copyCommand";
import { cn } from "@/lib/utils";

/*
 * The three page states (docs/blueprint/27 "States", D131). `framed` is for a
 * page whose content sits directly under its PageFrame, so the state lines up
 * with the title; inside `Page` or a section the column already does that.
 */
function StateColumn({
  framed,
  className,
  testId,
  children,
}: {
  framed: boolean;
  className?: string;
  testId?: string;
  children: ReactNode;
}) {
  return framed ? (
    <ModeFrame data-testid={testId} className={cn("px-5 pt-6", className)}>
      {children}
    </ModeFrame>
  ) : (
    <div data-testid={testId} className={className}>
      {children}
    </div>
  );
}

/** A skeleton that has not been wanted for this long is never drawn. */
const SKELETON_DELAY_MS = 150;

function useAfterDelay(delayMs: number): boolean {
  const [elapsed, setElapsed] = useState(false);
  useEffect(() => {
    const timer = setTimeout(() => setElapsed(true), delayMs);
    return () => clearTimeout(timer);
  }, [delayMs]);
  return elapsed;
}

const BAR_WIDTHS = ["w-2/5", "w-1/3", "w-1/2", "w-2/5", "w-3/5", "w-1/3"];
const LINE_WIDTHS = ["w-3/4", "w-2/3", "w-4/5", "w-1/2", "w-3/4", "w-3/5"];

function Bar({ className, delay }: { className: string; delay: number }) {
  return (
    <span
      aria-hidden
      className={cn("block h-3 animate-pulse bg-muted", className)}
      style={{ animationDelay: `${delay}ms` }}
    />
  );
}

/**
 * The page's own rows with bars where the words go, so the shape matches what
 * replaces it. Drawn only once a load has taken 150ms; a page that answers
 * from cache never flashes one. A page that already waits on its own timer
 * (`useDelayedPending`, which also holds a skeleton long enough not to
 * flicker) passes `shown` instead.
 */
export function PageSkeleton({
  rows = 8,
  label = "Loading",
  lead = "none",
  shown,
  framed = false,
}: {
  rows?: number;
  label?: string;
  /** What the real rows lead with: a person's avatar, or nothing. */
  lead?: "avatar" | "none";
  shown?: boolean;
  framed?: boolean;
}) {
  const delayed = useAfterDelay(SKELETON_DELAY_MS);
  const visible = shown ?? delayed;
  return (
    <StateColumn framed={framed} className={cn("overflow-hidden", framed && "px-0 pt-0")}>
      <ul
        role="status"
        aria-busy="true"
        aria-label={label}
        data-testid={visible ? "page-skeleton" : undefined}
        className={cn(!framed && "-mx-2")}
      >
        {visible
          ? Array.from({ length: rows }, (_, index) => (
              <Row
                key={index}
                lit={false}
                aria-hidden
                lead={lead === "avatar" ? <AvatarSkeleton /> : undefined}
                primary={
                  <Bar className={BAR_WIDTHS[index % BAR_WIDTHS.length]!} delay={index * 40} />
                }
                secondary={
                  <Bar
                    className={cn(LINE_WIDTHS[index % LINE_WIDTHS.length]!, "opacity-70")}
                    delay={index * 40}
                  />
                }
                trail={<Bar className="w-10 opacity-60" delay={index * 40} />}
              />
            ))
          : null}
      </ul>
    </StateColumn>
  );
}

function ErrorAction({ fix, onRetry }: { fix: ErrorFix; onRetry?: () => void }) {
  switch (fix) {
    case "retry":
      return onRetry ? (
        <Button variant="outline" size="sm" onClick={onRetry}>
          Try again
        </Button>
      ) : null;
    case "inbox":
      return (
        <Button asChild variant="outline" size="sm">
          <Link to="/m/$mailbox" params={{ mailbox: "inbox" }}>
            Go to Inbox
          </Link>
        </Button>
      );
    case "start":
      return (
        <Button variant="outline" size="sm" onClick={() => copyCommand(START_COMMAND)}>
          Copy start command
        </Button>
      );
    case "update":
      return (
        <Button variant="outline" size="sm" onClick={() => copyCommand(UPDATE_COMMAND)}>
          Copy update command
        </Button>
      );
    case "token":
      return (
        <Button asChild variant="outline" size="sm">
          <Link to="/settings/$section" params={{ section: "token" }}>
            Open token settings
          </Link>
        </Button>
      );
    case "none":
      return null;
  }
}

/**
 * What happened in plain words, what still works, and the one fix that can
 * work. The raw error is under Details. `title` names what failed when the
 * error itself is one nobody mapped; a mapped error says its own sentence.
 */
export function PageError({
  title,
  error,
  onRetry,
  subject,
  framed = false,
}: {
  title?: string;
  error: unknown;
  onRetry?: () => void;
  /** What was being asked for ("this conversation"), for a lookup that can't succeed. */
  subject?: string;
  framed?: boolean;
}) {
  const explanation = explainError(error, subject);
  const heading = explanation.generic && title ? title : explanation.heading;
  return (
    <StateColumn framed={framed} testId="page-error" className="flex flex-col items-start gap-1">
      <div role="alert" className="flex min-w-0 flex-col items-start gap-1">
        <h2 className="type-heading text-text-1">{heading}</h2>
        <p className="type-body max-w-[65ch] text-text-2">{explanation.body}</p>
        <div className="mt-3 empty:hidden">
          <ErrorAction fix={explanation.fix} onRetry={onRetry} />
        </div>
        <details className="mt-3 max-w-full">
          <summary className="type-meta w-fit cursor-pointer text-text-3 underline underline-offset-4">
            Details
          </summary>
          <pre className="type-meta mt-1 whitespace-pre-wrap break-words text-text-2">
            {explanation.technical}
          </pre>
        </details>
      </div>
    </StateColumn>
  );
}

/**
 * Nothing here, said once: what isn't here, the next useful place, and the
 * action that fills it if there is one. No icon, except a mode's own glyph.
 */
export function PageEmpty({
  title,
  body,
  action,
  glyph,
  moment,
  framed = false,
}: {
  title: string;
  body?: ReactNode;
  action?: ReactNode;
  glyph?: ModeGlyphId;
  /** A finishing scene drawn above the words (the low tide). It belongs to the page, not to this state. */
  moment?: ReactNode;
  framed?: boolean;
}) {
  const Glyph = glyph ? MODE_GLYPHS[glyph] : null;
  return (
    <StateColumn framed={framed} testId="page-empty" className="flex flex-col items-start gap-1">
      {moment ? <div className="mb-4">{moment}</div> : null}
      {Glyph ? <Glyph aria-hidden className="mb-2 size-6 text-text-3" /> : null}
      <h2 className="type-heading text-text-1">{title}</h2>
      {body ? <p className="type-body max-w-[65ch] text-text-2">{body}</p> : null}
      {action ? <div className="mt-3">{action}</div> : null}
    </StateColumn>
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
      <div className="type-body truncate text-text-1">{title}</div>
      {meta ? <div className="type-meta truncate text-text-2">{meta}</div> : null}
    </>
  );
  return (
    <li
      aria-current={current ? "true" : undefined}
      className={cn(
        // The lamp, not a tint: the current row is the one lit thing (D124).
        "group relative flex items-center gap-3 px-3 py-2.5",
        current
          ? "bg-ink-lamp before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:bg-signal"
          : "hover:bg-ink-lamp/50",
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
      {aside ? <div className="type-meta shrink-0 text-text-3">{aside}</div> : null}
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
          <dd className="min-w-0 truncate text-right text-sm tabular-nums">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

/** Explanatory prose under a section title; not an alert box. */
export function PageNote({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <p className={cn("mb-3 max-w-[65ch] text-xs text-muted-foreground", className)}>{children}</p>
  );
}
