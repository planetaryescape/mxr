import { createFileRoute } from "@tanstack/react-router";

import { TodoRoute } from "@/features/todo/TodoRoute";
import { optionalEnum, optionalString } from "@/lib/searchParams";

const TODO_VIEWS = ["catchup", "expired"] as const;

export interface TodoRouteParams {
  view?: (typeof TODO_VIEWS)[number];
  row?: string;
  source?: string;
}

export const Route = createFileRoute("/todo")({
  validateSearch: (search: Record<string, unknown>): TodoRouteParams => ({
    view: optionalEnum(search.view, TODO_VIEWS),
    row: optionalString(search.row),
    source: optionalString(search.source),
  }),
  component: Todo,
});

function Todo() {
  const { view } = Route.useSearch();
  return <TodoRoute view={view} />;
}
