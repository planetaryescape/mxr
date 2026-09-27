import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

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
        <div className={cn("mx-auto flex w-full items-end gap-4 px-6 pb-3 pt-5", max)}>
          <div className="min-w-0 flex-1">
            {eyebrow ? (
              <div className="mb-1 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
                {eyebrow}
              </div>
            ) : null}
            <h1 className="truncate text-xl font-semibold tracking-tight">{title}</h1>
            {description ? (
              <p className="mt-1 text-[13px] text-muted-foreground">{description}</p>
            ) : null}
          </div>
          {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
        </div>
        {tabs ? <div className={cn("mx-auto w-full px-6", max)}>{tabs}</div> : null}
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto" tabIndex={-1}>
        <div className={cn("mx-auto w-full px-6 py-5", max, className)}>{children}</div>
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
          <h2 className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
            {title}
          </h2>
          {description ? (
            <p className="mt-0.5 text-[12.5px] text-muted-foreground">{description}</p>
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
            "border-b-2 pb-2 pt-1 text-[13px] transition-colors",
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
