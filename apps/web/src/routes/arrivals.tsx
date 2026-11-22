import { createFileRoute } from "@tanstack/react-router";

import { ArrivalsRoute } from "@/features/arrivals/ArrivalsRoute";
import { ARRIVAL_BUCKETS, type ArrivalBucket } from "@/features/arrivals/types";
import { optionalEnum, optionalString } from "@/lib/searchParams";

interface ArrivalsSearch {
  bucket?: ArrivalBucket;
  since?: string;
  until?: string;
}

export const Route = createFileRoute("/arrivals")({
  validateSearch: (search: Record<string, unknown>): ArrivalsSearch => ({
    bucket: optionalEnum(search.bucket, ARRIVAL_BUCKETS),
    since: optionalString(search.since),
    until: optionalString(search.until),
  }),
  component: Arrivals,
});

function Arrivals() {
  const search = Route.useSearch();
  return <ArrivalsRoute {...search} />;
}
