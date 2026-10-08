import { Link } from "@tanstack/react-router";
import { CircleHelp } from "lucide-react";

import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

import { useArrivalsQuery } from "./api";
import { countLink, lineParts, linkedCounts, shownLine } from "./arrivalLine";
import { NotSureSection } from "./NotSureSection";
import type { Arrivals } from "./types";

/**
 * Sorting shows its work (D119), under Now's headline: where every email
 * since Now was last opened went, each count opening those emails; the
 * "Not sure" questions; and, once you have moved something, the week's
 * track record. Quiet on purpose: muted text, no badge, no colour.
 */
export function ArrivalsSection({ nowWaiting }: { nowWaiting: boolean }) {
  const arrivals = useArrivalsQuery();
  if (!arrivals.data) return null;
  return <ArrivalsView arrivals={arrivals.data} nowWaiting={nowWaiting} />;
}

export function ArrivalsView({
  arrivals,
  nowWaiting,
}: {
  arrivals: Arrivals;
  nowWaiting: boolean;
}) {
  return (
    <div data-testid="arrivals" className="shrink-0 border-b border-border px-5 py-2">
      <ArrivalsLine arrivals={arrivals} nowWaiting={nowWaiting} />
      {arrivals.track_record ? (
        <p data-testid="arrivals-track-record" className="mt-0.5 text-[12px] text-muted-foreground">
          {arrivals.track_record}
        </p>
      ) : null}
      {/* Always mounted: answering the last question refetches an empty
          list, and "Always for this sender?" lives in this component. */}
      <NotSureSection
        questions={arrivals.not_sure ?? []}
        heading={arrivals.not_sure_line ?? undefined}
        hint={arrivals.not_sure_hint ?? undefined}
      />
    </div>
  );
}

export function ArrivalsLine({
  arrivals,
  nowWaiting,
}: {
  arrivals: Arrivals;
  nowWaiting: boolean;
}) {
  const line = shownLine(arrivals, nowWaiting);
  const parts = lineParts(line, linkedCounts(arrivals));
  return (
    <p
      data-testid="arrivals-line"
      className="flex flex-wrap items-baseline gap-x-1 text-[12.5px] text-muted-foreground"
    >
      <span>
        {parts.map((part) =>
          "text" in part ? (
            <span key={part.at}>{part.text}</span>
          ) : (
            <Link
              key={part.at}
              to="/arrivals"
              search={countLink(arrivals, part.count)}
              data-testid={`arrivals-count-${part.count.bucket}`}
              className="underline decoration-border underline-offset-2 hover:text-foreground hover:decoration-current"
            >
              {part.count.label}
            </Link>
          ),
        )}
      </span>
      <Popover>
        <PopoverTrigger
          aria-label="Which mail is never sorted away"
          className="inline-flex size-5 items-center justify-center self-center rounded-sm hover:text-foreground"
        >
          <CircleHelp className="size-3.5" />
        </PopoverTrigger>
        <PopoverContent
          align="start"
          data-testid="arrivals-never-bury"
          className="text-[12.5px] leading-snug"
        >
          {arrivals.never_bury}
        </PopoverContent>
      </Popover>
    </p>
  );
}
