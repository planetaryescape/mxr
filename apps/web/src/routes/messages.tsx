import { createFileRoute } from "@tanstack/react-router";

import { DESK_LANES, type DeskLaneKind } from "@/features/desk/api";
import { DeskRoute } from "@/features/desk/DeskRoute";
import { optionalEnum } from "@/lib/searchParams";

export interface MessagesRouteParams {
  /** One lane in full, e.g. everyone you are waiting on. */
  lane?: DeskLaneKind;
}

/** Messages, an early version built on the desk's lanes (blueprint 22). */
export const Route = createFileRoute("/messages")({
  validateSearch: (search: Record<string, unknown>): MessagesRouteParams => ({
    lane: optionalEnum(search.lane, DESK_LANES),
  }),
  component: Messages,
});

function Messages() {
  const { lane } = Route.useSearch();
  return <DeskRoute lane={lane} mode="messages" />;
}
