import { createFileRoute } from "@tanstack/react-router";

import { SearchResultsRoute } from "@/features/search/SearchResultsRoute";
import { optionalEnum, optionalString } from "@/lib/searchParams";

export interface SearchRouteParams {
  q?: string;
  mode?: "lexical" | "semantic" | "hybrid";
  account?: string;
  sort?: "relevance" | "newest" | "oldest" | "verdict";
  scope?: "threads" | "messages" | "attachments" | "triage";
  verdict?: "ACTION" | "FYI" | "ROUTINE";
  groupBy?: "from" | "list" | "category";
}

export const Route = createFileRoute("/search")({
  validateSearch: (search: Record<string, unknown>): SearchRouteParams => ({
    q: optionalString(search.q),
    mode: optionalEnum(search.mode, ["lexical", "semantic", "hybrid"] as const),
    account: optionalString(search.account),
    sort: optionalEnum(search.sort, ["relevance", "newest", "oldest", "verdict"] as const),
    scope: optionalEnum(search.scope, ["threads", "messages", "attachments", "triage"] as const),
    verdict: optionalEnum(search.verdict, ["ACTION", "FYI", "ROUTINE"] as const),
    groupBy: optionalEnum(search.groupBy, ["from", "list", "category"] as const),
  }),
  component: SearchResultsRoute,
});
