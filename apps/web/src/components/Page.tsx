import type { ReactNode } from "react";

import { PageTitle } from "@/components/ModeFrame";
import { cn } from "@/lib/utils";
import { SectionHeading } from "@/components/SectionHeading";

/**
 * Layout for every non-mail page (analytics, rules, accounts, settings,
 * diagnostics…): one header pattern, one scroll container, one reading
 * width. Mail lists use ListWithReader instead.
 */
export function Page({
  title,
  description,
  eyebrow,
  actions,
  tabs,
  width = "default",
  children,
  className,
}: {
  title: string;
  description?: ReactNode;
  /** Mono label above the title, e.g. a section name. */
  eyebrow?: string;
  actions?: ReactNode;
  /** Tabs or filters that belong to the header. */
  tabs?: ReactNode;
  /** "default" 72rem, "narrow" 44rem for forms, "full" for dashboards. */
  width?: "narrow" | "default" | "full";
  children: ReactNode;
  className?: string;
}) {
  const max =
    width === "narrow" ? "max-w-[44rem]" : width === "full" ? "max-w-none" : "max-w-[72rem]";
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
      <header className="shrink-0 border-b border-border">
        <div className={cn("mx-auto flex w-full items-end gap-4 px-5 pb-3 pt-4", max)}>
          <div className="min-w-0 flex-1">
            {eyebrow ? (
              <div className="mb-1 font-mono text-2xs uppercase tracking-[0.12em] text-muted-foreground">
                {eyebrow}
              </div>
            ) : null}
            <PageTitle>{title}</PageTitle>
            {description ? (
              <p className="mt-1 text-sm text-muted-foreground">{description}</p>
            ) : null}
          </div>
          {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
        </div>
        {tabs ? <div className={cn("mx-auto w-full px-5", max)}>{tabs}</div> : null}
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto" tabIndex={-1}>
        <div className={cn("mx-auto w-full px-5 py-5", max, className)}>{children}</div>
      </div>
    </div>
  );
}

/** A titled block within a page, ruled rather than boxed (DESIGN.md: no cards). */
export function PageSection({
  title,
  description,
  actions,
  children,
  className,
}: {
  title: string;
  description?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("mb-8", className)}>
      <div className="mb-3 flex items-end justify-between gap-4 border-b border-border pb-1.5">
        <div className="min-w-0">
          <SectionHeading>{title}</SectionHeading>
          {description ? (
            <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
          ) : null}
        </div>
        {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
      </div>
      {children}
    </section>
  );
}

/** Tab strip for page headers: real tabs semantics, underlined active tab. */
export function PageTabs<T extends string>({
  value,
  tabs,
  onChange,
  label,
}: {
  value: T;
  tabs: { id: T; label: string; count?: number }[];
  onChange: (id: T) => void;
  label: string;
}) {
  return (
    <div role="tablist" aria-label={label} className="-mb-px flex gap-5">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          role="tab"
          aria-selected={value === tab.id}
          onClick={() => onChange(tab.id)}
          className={cn(
            "border-b-2 pb-2 pt-1 text-sm",
            value === tab.id
              ? "border-primary font-medium text-foreground"
              : "border-transparent text-muted-foreground hover:text-foreground",
          )}
        >
          {tab.label}
          {typeof tab.count === "number" ? (
            <span className="ml-1.5 font-mono text-2xs text-muted-foreground">{tab.count}</span>
          ) : null}
        </button>
      ))}
    </div>
  );
}
