import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

/** A count on an icon or a tab: round, mono, in the accent. */
export function CountBadge({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        "rounded-full bg-primary px-1 font-mono text-2xs leading-4 text-primary-foreground tabular-nums",
        className,
      )}
    >
      {children}
    </span>
  );
}
