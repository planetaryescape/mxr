import type { ComponentPropsWithoutRef } from "react";

import { cn } from "@/lib/utils";

/**
 * A small filled dot: sync health, a label's colour, "your turn". Round
 * shapes are for people and dots only (D127, check-shape.mjs), so every dot
 * is drawn here. The caller sets the size and colour (`size-2 bg-primary`).
 */
export function StatusDot({ className, ...rest }: ComponentPropsWithoutRef<"span">) {
  return (
    <span aria-hidden {...rest} className={cn("inline-block shrink-0 rounded-full", className)} />
  );
}
