import { createFileRoute } from "@tanstack/react-router";

import { TodoRoute } from "@/features/todo/TodoRoute";
import { optionalEnum } from "@/lib/searchParams";

const TODO_VIEWS = ["catchup", "expired"] as const;

export interface TodoRouteParams {
  /** The one-time catch-up, or the Expired list; omitted is the runway. */
  view?: (typeof TODO_VIEWS)[number];
}

export const Route = createFileRoute("/todo")({
  validateSearch: (search: Record<string, unknown>): TodoRouteParams => ({
    view: optionalEnum(search.view, TODO_VIEWS),
  }),
  component: Todo,
});

function Todo() {
  const { view } = Route.useSearch();
  return <TodoRoute view={view} />;
}
