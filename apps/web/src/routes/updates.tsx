import { createFileRoute } from "@tanstack/react-router";

import { UpdatesRoute } from "@/features/updates/UpdatesRoute";
import { optionalString } from "@/lib/searchParams";

export interface UpdatesRouteParams {
  row?: string;
  peek?: string;
}

export const Route = createFileRoute("/updates")({
  validateSearch: (search: Record<string, unknown>): UpdatesRouteParams => ({
    row: optionalString(search.row),
    peek: optionalString(search.peek),
  }),
  component: UpdatesRoute,
});
