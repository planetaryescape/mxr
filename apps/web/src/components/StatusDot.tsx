import type { ComponentPropsWithoutRef } from "react";

import { cn } from "@/lib/utils";

export function StatusDot({ className, ...rest }: ComponentPropsWithoutRef<"span">) {
  return (
    <span aria-hidden {...rest} className={cn("inline-block shrink-0 rounded-full", className)} />
  );
}
