import { Link } from "@tanstack/react-router";

import { Chip } from "@/components/Chip";
import { KeyChip } from "@/components/KeyChip";
import { cn } from "@/lib/utils";

import { otherModes, type ModeKind, type ThreadModes } from "./membership";
import { RAIL_PATHS } from "./rail";

export function AlsoInLine({
  modes,
  here,
  className,
  keys = true,
  links = true,
  chips = false,
}: {
  modes: ThreadModes | null | undefined;
  here: ModeKind | null;
  className?: string;
  keys?: boolean;
  links?: boolean;
  chips?: boolean;
}) {
  const others = otherModes(modes, here);
  if (others.length === 0) return null;
  return (
    <p
      data-testid="also-in"
      className={cn("flex flex-wrap gap-x-3 gap-y-0.5 text-xs text-muted-foreground", className)}
    >
      {others.map((entry) => (
        <span key={entry.mode} className="inline-flex min-w-0 items-center gap-1">
          {chips && links ? (
            <Chip asChild>
              <Link to={RAIL_PATHS[entry.mode] ?? "/now"} title={entry.reason}>
                {entry.also_in}
              </Link>
            </Chip>
          ) : links ? (
            <Link
              to={RAIL_PATHS[entry.mode] ?? "/now"}
              title={entry.reason}
              className="flex min-h-6 min-w-0 items-center truncate underline decoration-border-strong underline-offset-4 hover:text-foreground hover:decoration-foreground"
            >
              {entry.also_in}
            </Link>
          ) : (
            <span title={entry.reason} className="min-w-0 truncate">
              {entry.also_in}
            </span>
          )}
          {keys ? <KeyChip className="h-4 px-1">{entry.key}</KeyChip> : null}
        </span>
      ))}
    </p>
  );
}
