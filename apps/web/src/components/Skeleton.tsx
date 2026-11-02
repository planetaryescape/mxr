import type { ReactNode } from "react";

import { AvatarSkeleton } from "@/components/Avatar";
import { Row } from "@/components/Row";
import { cn } from "@/lib/utils";

/** Presentation only: no placeholder links, controls or invented values. */
export function Skeleton({
  shown,
  label,
  className,
  children,
}: {
  shown: boolean;
  label: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      role="status"
      aria-busy="true"
      aria-label={label}
      data-testid={shown ? "page-skeleton" : undefined}
      className={className}
    >
      <div aria-hidden className={cn("contents", !shown && "invisible")}>
        {children}
      </div>
    </div>
  );
}

export function SkeletonBar({ className }: { className?: string }) {
  return <span aria-hidden className={cn("block h-3 animate-pulse bg-muted", className)} />;
}

export interface SkeletonRowProps {
  lead?: "none" | "avatar" | "checkbox" | "source" | "amount";
  secondary?: boolean;
  metadataClassName?: string;
}

/** Keep the shared row height and line boxes, with the mode's actual lead. */
export function SkeletonRow({
  lead = "none",
  secondary = true,
  metadataClassName,
}: SkeletonRowProps) {
  const mark =
    lead === "avatar" ? (
      <AvatarSkeleton size="lg" />
    ) : lead === "none" ? undefined : (
      <SkeletonBar
        className={cn(
          "size-4 shrink-0",
          lead === "checkbox" && "border border-rule bg-transparent",
          lead === "amount" && "w-16",
        )}
      />
    );
  return (
    <Row
      lit={false}
      aria-hidden
      data-skeleton-row={lead}
      lead={mark}
      primary={
        <span className="flex h-5 items-center">
          <SkeletonBar className="w-2/5" />
        </span>
      }
      secondary={
        secondary ? (
          <span className="flex h-5 items-center">
            <SkeletonBar className="w-3/4 opacity-70" />
          </span>
        ) : undefined
      }
      time={<SkeletonBar className={cn("w-10 opacity-60", metadataClassName)} />}
    />
  );
}
