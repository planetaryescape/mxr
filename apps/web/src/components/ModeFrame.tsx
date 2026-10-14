import type { ComponentPropsWithoutRef, ReactNode } from "react";

import { cn } from "@/lib/utils";
import { usePublishPageTitle } from "@/state/pageTitleStore";

/**
 * The page grid every mode page shares (styles in app.css, widths in
 * tokens.css). "list" (72rem) for rows, "reader" (980px) for a conversation,
 * "wide" for a page that splits into `ModeColumns` on a wide screen. The
 * frame must sit inside a `mode-page` container for "wide" to widen.
 */
export type FrameWidth = "list" | "reader" | "wide";

export function ModeFrame({
  width = "list",
  className,
  children,
  ...rest
}: { width?: FrameWidth; children: ReactNode } & ComponentPropsWithoutRef<"div">) {
  return (
    <div {...rest} data-frame={width} className={cn("mode-frame", className)}>
      {children}
    </div>
  );
}

/**
 * The one page title: the same size and weight in every mode and in Page.
 * It publishes itself so the Topbar drops the crumb that repeats it.
 */
export function PageTitle({ children, className }: { children: string; className?: string }) {
  usePublishPageTitle(children);
  return (
    <h1
      className={cn(
        "min-w-0 truncate text-lg font-semibold tracking-tight text-foreground",
        className,
      )}
    >
      {children}
    </h1>
  );
}

/**
 * A mode page's header band: full-width rule, content in the page's frame,
 * so the title lines up with the rows under it. `title`, `subtitle` and
 * `actions` fill the title row; `children` is anything below it.
 */
export function ModeHeader({
  width = "list",
  className,
  title,
  subtitle,
  actions,
  children,
}: {
  width?: FrameWidth;
  className?: string;
  title?: string;
  /** One line under the title, in the mode's own words. */
  subtitle?: ReactNode;
  /** Right-aligned in the title row. */
  actions?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <header className="shrink-0 border-b border-border">
      <ModeFrame width={width} className={cn("px-5 pb-3 pt-4", className)}>
        {title ? (
          <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <PageTitle>{title}</PageTitle>
            {subtitle ? (
              <p data-testid="mode-header" className="min-w-0 text-xs text-muted-foreground">
                {subtitle}
              </p>
            ) : null}
            {actions ? <div className="ml-auto flex items-center gap-3">{actions}</div> : null}
          </div>
        ) : null}
        {children}
      </ModeFrame>
    </header>
  );
}

/**
 * Two columns on a wide screen, one otherwise. Children read top to bottom,
 * left column first, which is also the keyboard order. `split` false keeps
 * one column, for when one side would be empty.
 */
export function ModeColumns({ split, children }: { split: boolean; children: ReactNode }) {
  return (
    <div className="mode-columns" data-split={split ? "true" : "false"}>
      {children}
    </div>
  );
}
