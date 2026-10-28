import { createFileRoute } from "@tanstack/react-router";

import { ReadingReader } from "@/features/reading/ReadingReader";
import { optionalEnum } from "@/lib/searchParams";

export interface ReadingItemParams {
  /** The issue as sent, or the linked article; omitted picks for the item. */
  as?: "issue" | "article";
  /** Opened from the Highlights tab (`3`): start at the item's highlights. */
  at?: "highlights";
}

export const Route = createFileRoute("/reading/item/$itemKey")({
  validateSearch: (search: Record<string, unknown>): ReadingItemParams => ({
    as: optionalEnum(search.as, ["issue", "article"] as const),
    at: optionalEnum(search.at, ["highlights"] as const),
  }),
  component: OpenItem,
});

function OpenItem() {
  const { itemKey } = Route.useParams();
  return <ReadingReader itemKey={itemKey} />;
}
