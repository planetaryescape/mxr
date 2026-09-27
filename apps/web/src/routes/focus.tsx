import { createFileRoute } from "@tanstack/react-router";

import { FocusRoute } from "@/features/focus/FocusRoute";
import { optionalString } from "@/lib/searchParams";

export interface FocusRouteParams {
  /** The page to return to on Esc. */
  from?: string;
  /** A desk lane that opened focus mode; reserved, the queue is the same. */
  lane?: string;
}

export const Route = createFileRoute("/focus")({
  validateSearch: (search: Record<string, unknown>): FocusRouteParams => ({
    from: optionalString(search.from),
    lane: optionalString(search.lane),
  }),
  component: FocusPage,
});

function FocusPage() {
  const { from } = Route.useSearch();
  return <FocusRoute from={from} />;
}
