import { cn } from "@/lib/utils";

const PREFIX = "Here because:";

export function reasonText(reason: string): string {
  const text = reason.trim();
  return text.startsWith(PREFIX) ? text : `${PREFIX} ${text}`;
}

export function ReasonLine({ reason, className }: { reason: string; className?: string }) {
  return (
    <p data-testid="reason-line" className={cn("type-meta text-text-2", className)}>
      {reasonText(reason)}
    </p>
  );
}
