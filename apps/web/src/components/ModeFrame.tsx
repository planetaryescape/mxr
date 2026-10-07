import type { ComponentPropsWithoutRef, ReactNode } from "react";

import { cn } from "@/lib/utils";

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
 * A mode page's header band: full-width rule, content in the page's frame,
 * so the title lines up with the rows under it.
 */
export function ModeHeader({
  width = "list",
  className,
  children,
}: {
  width?: FrameWidth;
  className?: string;
  children: ReactNode;
}) {
  return (
    <header className="shrink-0 border-b border-border">
      <ModeFrame width={width} className={cn("px-5 pb-3 pt-4", className)}>
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
