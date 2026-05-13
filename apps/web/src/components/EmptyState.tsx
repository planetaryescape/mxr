import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

interface EmptyStateProps {
  icon?: LucideIcon;
  title: string;
  description?: string;
  action?: ReactNode;
  className?: string;
  role?: "alert" | "status";
}

export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  className,
  role,
}: EmptyStateProps) {
  return (
    <div
      role={role}
      className={cn(
        "flex h-full w-full flex-col items-center justify-center gap-3 px-6 text-center",
        className,
      )}
    >
      {Icon ? (
        <div className="flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
          <Icon className="size-4" />
        </div>
      ) : null}
      <div>
        <div className="text-md font-medium text-foreground">{title}</div>
        {description ? (
          <div className="mt-1 max-w-sm text-xs text-muted-foreground">{description}</div>
        ) : null}
      </div>
      {action}
    </div>
  );
}
