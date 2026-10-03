import { plural } from "@/lib/format";

import { useRailQuery } from "./rail";

/**
 * The line under an early mode's name: what the mode is for, and that this
 * view is an early version built on an existing one, in the rail's words.
 */
export function EarlyModeNote({ mode }: { mode: string }) {
  const entry = useRailQuery().data?.entries.find((candidate) => candidate.id === mode);
  if (!entry) return null;
  return (
    <div data-testid="early-mode-note" className="px-5 pt-3 text-[12.5px] text-muted-foreground">
      {entry.header ? <p data-testid="mode-header">{entry.header}</p> : null}
      {entry.status === "early" ? (
        <p className="mt-0.5 flex flex-wrap items-baseline gap-x-2">
          <span
            data-testid="early-version"
            className="rounded border border-border px-1 font-mono text-[10px] uppercase tracking-wide"
          >
            early version
          </span>
          {entry.early_note ? <span>{entry.early_note}</span> : null}
        </p>
      ) : null}
      {entry.quiet ? (
        <p data-testid="quiet-line" className="mt-0.5">
          {plural(entry.quiet, "quiet conversation")}: person mail still in your inbox that no lane
          holds. They stay in Messages until you mark them done here, and are not in the count
          beside Messages.
        </p>
      ) : null}
    </div>
  );
}
