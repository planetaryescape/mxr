import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { HTMLAttributes } from "react";

import { cn } from "@/lib/utils";

const chipVariants = cva(
  "inline-flex h-5 shrink-0 items-center gap-1 rounded-md border px-1.5 text-2xs font-medium whitespace-nowrap [&_svg]:pointer-events-none [&_svg]:size-3",
  {
    variants: {
      tone: {
        neutral: "border-border text-muted-foreground",
        attention: "border-warning/40 bg-warning/10 text-warning",
        success: "border-success/40 bg-success/10 text-success",
      },
    },
    defaultVariants: { tone: "neutral" },
  },
);

export interface ChipProps extends HTMLAttributes<HTMLElement>, VariantProps<typeof chipVariants> {
  asChild?: boolean;
}

export function Chip({ asChild, tone, className, ...props }: ChipProps) {
  const Component = asChild ? Slot : "span";
  return <Component className={cn(chipVariants({ tone }), className)} {...props} />;
}
