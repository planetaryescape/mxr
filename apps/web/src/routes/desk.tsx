import { createFileRoute } from "@tanstack/react-router";

import { DESK_LANES, type DeskLaneKind } from "@/features/desk/api";
import { DeskRoute } from "@/features/desk/DeskRoute";
import { optionalEnum } from "@/lib/searchParams";

export interface DeskRouteParams {
  /** One lane in full, e.g. everything you are waiting on. */
  lane?: DeskLaneKind;
}

export const Route = createFileRoute("/desk")({
  validateSearch: (search: Record<string, unknown>): DeskRouteParams => ({
    lane: optionalEnum(search.lane, DESK_LANES),
  }),
  component: Desk,
});

function Desk() {
  const { lane } = Route.useSearch();
  return <DeskRoute lane={lane} />;
}
