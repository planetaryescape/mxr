import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

const LABEL = "type-eyebrow text-text-3";

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
  className?: string;
  children: ReactNode;
}) {
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
