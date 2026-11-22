import type { ComponentPropsWithoutRef } from "react";

import { StatusDot } from "@/components/StatusDot";
import { cn } from "@/lib/utils";

export function UnreadDot({ className, ...rest }: ComponentPropsWithoutRef<"span">) {
  return <StatusDot {...rest} className={cn("size-1.5 bg-unread-marker", className)} />;
}
