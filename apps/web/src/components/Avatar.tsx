import type { ReactNode } from "react";

import { avatarTone } from "@/components/avatarTone";
import { initials } from "@/lib/format";
import { cn } from "@/lib/utils";

const SIZES = {
  sm: "size-5 text-2xs",
  md: "size-7 text-2xs [[data-density=compact]_&]:size-6",
  lg: "size-8 text-xs",
} as const;

type AvatarSize = keyof typeof SIZES;

/**
 * A sender's initials on their stable hue. Inbox, Messages and the thread
 * all draw it here, so one sender wears one colour everywhere. `lg` is the
 * Messages row and the thread card; `md` is a mail row; `sm` sits inside a
 * recipient chip. Round shapes are for people and dots only (D127).
 */
export function Avatar({
  name,
  size = "md",
  className,
}: {
  name: string;
  size?: AvatarSize;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "grid shrink-0 place-items-center rounded-full font-mono font-semibold",
        SIZES[size],
        avatarTone(name),
        className,
      )}
    >
      {initials(name)}
    </span>
  );
}

/** The neutral avatar for a group, which has no one sender to colour. */
export function AvatarGlyph({
  size = "md",
  children,
  className,
}: {
  size?: AvatarSize;
  children: ReactNode;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "grid shrink-0 place-items-center rounded-full bg-muted text-foreground/80",
        SIZES[size],
        className,
      )}
    >
      {children}
    </span>
  );
}

/** The placeholder for an avatar while its row loads. */
export function AvatarSkeleton({
  size = "md",
  className,
}: {
  size?: AvatarSize;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn("shrink-0 animate-pulse rounded-full bg-muted", SIZES[size], className)}
    />
  );
}
