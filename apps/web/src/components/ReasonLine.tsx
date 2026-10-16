import { cn } from "@/lib/utils";

const PREFIX = "Here because:";

/** The daemon's reason strings already begin "Here because:"; keep one prefix either way. */
export function reasonText(reason: string): string {
  const text = reason.trim();
  return text.startsWith(PREFIX) ? text : `${PREFIX} ${text}`;
}

/**
 * "Here because: ..." with the daemon's string (D111). The lit row's reason
 * shows in the status bar (via `useLitReason`) and under a detail page's
 * header, never as a third line inside a row.
 */
export function ReasonLine({ reason, className }: { reason: string; className?: string }) {
  return (
    <p data-testid="reason-line" className={cn("type-meta text-text-2", className)}>
      {reasonText(reason)}
    </p>
  );
}
