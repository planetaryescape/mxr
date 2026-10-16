import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

/** A count on an icon or a tab: round, mono, in the unread mark's colour (the accent; the text tier in Paper). */
export function CountBadge({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        "rounded-full bg-unread-marker px-1 font-mono text-2xs leading-4 text-unread-marker-foreground tabular-nums",
        className,
      )}
    >
      {children}
    </span>
  );
}
