import { Link } from "@tanstack/react-router";

import { KeyChip } from "@/components/KeyChip";
import { cn } from "@/lib/utils";

import { otherModes, type ModeKind, type ThreadModes } from "./membership";
import { RAIL_PATHS } from "./rail";

/**
 * "Also in To do: Sign the lease, act by Mon 13 Oct": the other modes
 * holding this thread, in the daemon's words, each a link to that mode with
 * its `g` key beside it. Nothing shows when no other mode holds it.
 */
export function AlsoInLine({
  modes,
  here,
  className,
  keys = true,
  links = true,
}: {
  modes: ThreadModes | null | undefined;
  /** The mode on screen, left out of the line. */
  here: ModeKind | null;
  className?: string;
  /** Show each mode's `g` key; rows leave it out to stay one line. */
  keys?: boolean;
  /**
   * Plain text inside a listbox option, which can't hold links (the row
   * opens on Enter and the mode's `g` key jumps there).
   */
  links?: boolean;
}) {
  const others = otherModes(modes, here);
  if (others.length === 0) return null;
  return (
    <p
      data-testid="also-in"
      className={cn(
        "flex flex-wrap gap-x-3 gap-y-0.5 text-[12px] text-muted-foreground",
        className,
      )}
    >
      {others.map((entry) => (
        <span key={entry.mode} className="inline-flex min-w-0 items-center gap-1">
          {links ? (
            <Link
              to={RAIL_PATHS[entry.mode] ?? "/now"}
              title={entry.reason}
              className="min-w-0 truncate underline decoration-border-strong underline-offset-4 hover:text-foreground hover:decoration-primary"
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
