import { createFileRoute } from "@tanstack/react-router";

import { FocusRoute } from "@/features/focus/FocusRoute";
import { optionalEnum, optionalString } from "@/lib/searchParams";

export interface FocusRouteParams {
  /** The page to return to on Esc. */
  from?: string;
  /** "owed": only the replies you owe (the desk's You owe lane), without
   * the reply-later queue. */
  lane?: "owed";
  /** The conversation on screen. */
  at?: string;
}

export const Route = createFileRoute("/focus")({
  validateSearch: (search: Record<string, unknown>): FocusRouteParams => ({
    from: optionalString(search.from),
    lane: optionalEnum(search.lane, ["owed"] as const),
    at: optionalString(search.at),
  }),
  component: FocusPage,
});

function FocusPage() {
  const { from, lane, at } = Route.useSearch();
  return <FocusRoute from={from} lane={lane} at={at} />;
}
