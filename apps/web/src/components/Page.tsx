import { useRef, type ReactNode } from "react";

import { PageFrame, type PageAction } from "@/components/PageFrame";
import { cn } from "@/lib/utils";
import { SectionHeading } from "@/components/SectionHeading";
import { choiceIndex } from "@/lib/keys/choiceNavigation";
import { KeyChip } from "@/components/KeyChip";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { useScopeController } from "@/lib/keys/controllers";
import { definedShortcut } from "@/lib/actions/definition";

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
  description?: ReactNode;
  eyebrow?: ReactNode;
  primary?: ReactNode;
  overflow?: readonly PageAction[];
  tabs?: ReactNode;
  freshness?: ReactNode;
  toolbar?: ReactNode;
  width?: "narrow" | "default" | "full";
  children: ReactNode;
  className?: string;
}) {
  const root = useRef<HTMLDivElement>(null);
  const hasControls = Boolean(tabs || toolbar);
  useShortcutScope("page", hasControls);
  useScopeController(
    "page",
    hasControls
      ? {
          controls: () => {
            root.current
              ?.querySelector<HTMLElement>(
                '[role="tab"][tabindex="0"], [role="radio"][tabindex="0"], [role="combobox"], nav a[aria-current="page"], button[aria-pressed]:not(:disabled), input:not([type="hidden"]):not(:disabled), select:not(:disabled)',
              )
              ?.focus();
          },
        }
      : {},
  );
  const max =
    width === "narrow" ? "max-w-[44rem]" : width === "full" ? "max-w-none" : "max-w-[72rem]";
  return (
    <div
      ref={root}
      data-page-pane=""
      tabIndex={-1}
      className="flex min-h-0 min-w-0 flex-1 flex-col bg-background outline-none"
    >
      <PageFrame
        eyebrow={eyebrow}
        title={title}
        purpose={description}
        primary={primary}
        overflow={overflow}
        tabs={tabs}
        tabsShortcut={tabs ? definedShortcut("page.controls") : undefined}
        freshness={freshness}
        width={width === "default" ? "list" : width}
      />
      <div className="min-h-0 flex-1 overflow-y-auto" tabIndex={-1}>
        <div className={cn("mx-auto w-full px-5 py-5", max, className)}>
          {toolbar ? (
            <div className="mb-4 flex flex-wrap items-center gap-2">
              {toolbar}
              {!tabs ? <KeyChip>{definedShortcut("page.controls")}</KeyChip> : null}
            </div>
          ) : null}
          {children}
        </div>
      </div>
    </div>
  );
}

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

export function pageTabClass(active: boolean): string {
  return cn(
    "type-body inline-flex h-8 shrink-0 items-center border-b-2 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-signal",
    active
      ? "border-signal font-semibold text-text-1"
      : "border-transparent text-text-2 hover:text-text-1",
  );
}

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
    <div role="tablist" aria-label={label} className="-mb-px flex h-8 items-center gap-6">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          role="tab"
          aria-selected={value === tab.id}
          tabIndex={value === tab.id ? 0 : -1}
          onClick={() => onChange(tab.id)}
          onKeyDown={(event) => {
            if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
            const next = choiceIndex(event.key, tabs.indexOf(tab), tabs.length);
            if (next === null) return;
            event.preventDefault();
            const target = tabs[next];
            if (!target) return;
            onChange(target.id);
            const buttons =
              event.currentTarget.parentElement?.querySelectorAll<HTMLElement>('[role="tab"]');
            buttons?.[next]?.focus();
          }}
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
