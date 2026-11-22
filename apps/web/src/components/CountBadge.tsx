import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

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
