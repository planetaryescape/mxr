import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

const LABEL = "type-eyebrow text-text-3";

/**
 * A section or band label: mono caps, an optional count and an optional
 * right-hand slot. `as` keeps the heading level the page's outline needs.
 * The count is shown at 0 too: an empty section should still say so.
 */
export function SectionHeading({
  as: Tag = "h2",
  id,
  count,
  actions,
  className,
  children,
}: {
  as?: "h2" | "h3" | "h4";
  id?: string;
  count?: number;
  actions?: ReactNode;
  /** Spacing and layout belong to the caller; the label recipe is fixed here. */
  className?: string;
  children: ReactNode;
}) {
  // With an actions slot the spacing moves to the row, so the slot sits on the label's line.
  const label = (
    <Tag id={id} className={actions ? LABEL : cn(LABEL, className)}>
      {children}
      {count !== undefined ? <span className="ml-2 tabular-nums">{count}</span> : null}
    </Tag>
  );
  if (!actions) return label;
  return (
    <div className={cn("flex items-baseline justify-between gap-3", className)}>
      {label}
      <div className="flex shrink-0 items-center gap-3">{actions}</div>
    </div>
  );
}
