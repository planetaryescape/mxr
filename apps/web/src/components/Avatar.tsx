import type { ReactNode } from "react";

import { avatarTone } from "@/components/avatarTone";
import { initials } from "@/lib/format";
import { cn } from "@/lib/utils";

const SIZES = {
  xs: "size-4 text-2xs",
  sm: "size-5 text-2xs",
  md: "size-7 text-2xs [[data-density=compact]_&]:size-6",
  lg: "size-8 text-xs",
} as const;

type AvatarSize = keyof typeof SIZES;

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
