import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-2xs font-medium whitespace-nowrap transition-colors [&_svg]:pointer-events-none [&_svg]:size-3",
  {
    variants: {
      variant: {
        default: "border-transparent bg-primary text-primary-foreground",
        secondary: "border-border bg-muted text-muted-foreground",
        outline: "border-border bg-background text-foreground",
        destructive: "border-destructive/30 bg-destructive/10 text-destructive",
        success: "border-success/30 bg-success/10 text-success",
        warning: "border-warning/30 bg-warning/10 text-warning",
      },
    },
    defaultVariants: {
      variant: "secondary",
    },
  },
);

interface BadgeProps
  extends React.HTMLAttributes<HTMLSpanElement>, VariantProps<typeof badgeVariants> {
  asChild?: boolean;
}

function Badge({ className, variant, asChild, ...props }: BadgeProps) {
  const Component = asChild ? Slot : "span";
  return <Component className={cn(badgeVariants({ variant }), className)} {...props} />;
}

export { Badge, badgeVariants };
