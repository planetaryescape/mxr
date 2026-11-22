import { createFileRoute } from "@tanstack/react-router";

import { FocusRoute } from "@/features/focus/FocusRoute";
import { optionalEnum, optionalString } from "@/lib/searchParams";

export interface FocusRouteParams {
  from?: string;
  lane?: "owed";
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
