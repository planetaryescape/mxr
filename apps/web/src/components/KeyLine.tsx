import type { ReactNode } from "react";

import { KeyChip } from "@/components/KeyChip";
import { cn } from "@/lib/utils";

export interface KeyHint {
  key: string;
  verb: ReactNode;
}

/**
 * The key hints at the foot of a mode page, so each key is learnt where it
 * works. Desktop only: phones have no keyboard. Each mode passes the keys it
 * shows; the filter is the mode's, not this component's.
 */
export function KeyLine({ keys, className }: { keys: KeyHint[]; className?: string }) {
  return (
    <p
      className={cn(
        "mx-5 mt-6 hidden flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground md:flex",
        className,
      )}
    >
      {keys.map((hint) => (
        <span key={hint.key} className="inline-flex items-center gap-1">
          <KeyChip>{hint.key === "Enter" ? "↵" : hint.key}</KeyChip> {hint.verb}
        </span>
      ))}
    </p>
  );
}
