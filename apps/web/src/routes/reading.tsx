import { createFileRoute } from "@tanstack/react-router";

import { ReadingRoute } from "@/features/reading/ReadingRoute";
import { optionalEnum, optionalString } from "@/lib/searchParams";

const READING_VIEWS = ["later"] as const;

export interface ReadingRouteParams {
  view?: (typeof READING_VIEWS)[number];
  row?: string;
}

export const Route = createFileRoute("/reading")({
  validateSearch: (search: Record<string, unknown>): ReadingRouteParams => ({
    view: optionalEnum(search.view, READING_VIEWS),
    row: optionalString(search.row),
  }),
  component: Reading,
});

function Reading() {
  const { view } = Route.useSearch();
  return <ReadingRoute view={view} />;
}
