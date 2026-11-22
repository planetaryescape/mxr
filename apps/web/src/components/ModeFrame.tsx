import type { ComponentPropsWithoutRef, ReactNode } from "react";

import { cn } from "@/lib/utils";

export type FrameWidth = "list" | "reader" | "reading" | "wide" | "narrow" | "full";

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

export function ModeColumns({ split, children }: { split: boolean; children: ReactNode }) {
  return (
    <div className="mode-columns" data-split={split ? "true" : "false"}>
      {children}
    </div>
  );
}
