import { createFileRoute } from "@tanstack/react-router";

import { NowRoute } from "@/features/now/NowRoute";
import { optionalString } from "@/lib/searchParams";

export interface NowRouteParams {
  row?: string;
}

export const Route = createFileRoute("/now")({
  validateSearch: (search: Record<string, unknown>): NowRouteParams => ({
    row: optionalString(search.row),
  }),
  component: NowRoute,
});
