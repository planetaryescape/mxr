import type { ReactNode } from "react";

import { PageFrame, type PageAction } from "@/components/PageFrame";
import { cn } from "@/lib/utils";
import { SectionHeading } from "@/components/SectionHeading";

/**
 * Layout for every non-mail page (analytics, rules, accounts, settings,
 * diagnostics…): the page frame, one scroll container and one reading
 * width. Mail lists use ListWithReader instead. The header slots are
 * PageFrame's; `toolbar` is for the filters and pickers that used to crowd
 * the header and now sit at the top of the content.
 */
export function Page({
  title,
  description,
  eyebrow,
  primary,
  overflow,
  tabs,
  freshness,
  toolbar,
  width = "default",
  children,
  className,
}: {
  title: string;
  /** The frame's purpose line. */
  description?: ReactNode;
  eyebrow?: ReactNode;
  primary?: ReactNode;
  overflow?: readonly PageAction[];
  /** Tabs or filters that belong to the header. */
  tabs?: ReactNode;
  freshness?: ReactNode;
  /** Pickers and filters, at the top of the content. */
  toolbar?: ReactNode;
  /** "default" 72rem, "narrow" 44rem for forms, "full" for dashboards. */
  width?: "narrow" | "default" | "full";
  children: ReactNode;
  className?: string;
}) {
  const max =
    width === "narrow" ? "max-w-[44rem]" : width === "full" ? "max-w-none" : "max-w-[72rem]";
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
      <PageFrame
        eyebrow={eyebrow}
        title={title}
        purpose={description}
        primary={primary}
        overflow={overflow}
        tabs={tabs}
        freshness={freshness}
        width={width === "default" ? "list" : width}
      />
      <div className="min-h-0 flex-1 overflow-y-auto" tabIndex={-1}>
        <div className={cn("mx-auto w-full px-5 py-5", max, className)}>
          {toolbar ? <div className="mb-4 flex flex-wrap items-center gap-2">{toolbar}</div> : null}
          {children}
        </div>
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

/** The look of one tab: Body, a signal underline when it is the page you are on. */
export function pageTabClass(active: boolean): string {
  return cn(
    "type-body inline-flex h-8 shrink-0 items-center border-b-2",
    active
      ? "border-signal font-semibold text-text-1"
      : "border-transparent text-text-2 hover:text-text-1",
  );
}

/** Tab strip for page headers: real tabs semantics, a signal underline on the active tab. */
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
    <div role="tablist" aria-label={label} className="-mb-px flex h-8 gap-6">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          role="tab"
          aria-selected={value === tab.id}
          onClick={() => onChange(tab.id)}
          className={pageTabClass(value === tab.id)}
        >
          {tab.label}
          {typeof tab.count === "number" ? (
            <span className="type-meta ml-1.5 text-text-3">{tab.count}</span>
          ) : null}
        </button>
      ))}
    </div>
  );
}
