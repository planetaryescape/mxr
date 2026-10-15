import { createFileRoute } from "@tanstack/react-router";

import { UpdatesRoute } from "@/features/updates/UpdatesRoute";
import { optionalString } from "@/lib/searchParams";

export interface UpdatesRouteParams {
  /** The line under the cursor. */
  row?: string;
  /** The line whose peek is open. */
  peek?: string;
}

/** Updates: notifications as a twice-daily briefing by source (blueprint 22). */
export const Route = createFileRoute("/updates")({
  validateSearch: (search: Record<string, unknown>): UpdatesRouteParams => ({
    row: optionalString(search.row),
    peek: optionalString(search.peek),
  }),
  component: UpdatesRoute,
});
