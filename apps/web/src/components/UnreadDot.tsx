import type { ComponentPropsWithoutRef } from "react";

import { StatusDot } from "@/components/StatusDot";
import { cn } from "@/lib/utils";

/** The mark on an unread row: a 6px dot in the unread colour. */
export function UnreadDot({ className, ...rest }: ComponentPropsWithoutRef<"span">) {
  return <StatusDot {...rest} className={cn("size-1.5 bg-unread-marker", className)} />;
}
