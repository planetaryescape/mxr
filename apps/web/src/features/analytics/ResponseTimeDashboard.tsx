import { useQuery } from "@tanstack/react-query";
import { Timer } from "lucide-react";
import { useState } from "react";

import { fetchResponseTime, rangeDays, type AnalyticsRange, type ResponseDirection } from "./api";
import { BarList, Segmented, formatDuration } from "./analyticsParts";
import { PageSection } from "@/components/Page";
import { FactList, PageEmpty, PageError, PageNote, PageSkeleton } from "@/components/PageParts";
import { plural } from "@/lib/format";

const DIRECTIONS: { id: ResponseDirection; label: string }[] = [
  { id: "they_replied", label: "Their replies" },
  { id: "i_replied", label: "My replies" },
];

/** Upper bound 2^32-1 marks the open-ended last bucket. */
const OPEN_ENDED = 4_294_967_295;

function bucketLabel(upper: number, previous: number | null): string {
  if (upper >= OPEN_ENDED) return `${formatDuration(previous ?? 0)} or more`;
  return `under ${formatDuration(upper)}`;
}

export function ResponseTimeDashboard({ range }: { range: AnalyticsRange }) {
  const [direction, setDirection] = useState<ResponseDirection>("they_replied");
  const response = useQuery({
    queryKey: ["analytics", "response", range, direction],
    queryFn: () => fetchResponseTime(rangeDays(range), direction),
  });
  const summary = response.data?.summary;
  const histogram = summary?.histogram ?? [];

  return (
    <PageSection
      title="Response time"
      actions={
        <Segmented
          value={direction}
          options={DIRECTIONS}
          onChange={setDirection}
          label="Reply direction"
        />
      }
    >
      <PageNote>
        {direction === "they_replied"
          ? "How long others take to answer mail you sent."
          : "How long you take to answer mail you received."}{" "}
        Business-hours figures ignore nights and weekends.
      </PageNote>
      {response.isPending ? (
        <PageSkeleton rows={4} label="Loading response time" />
      ) : response.isError ? (
        <PageError
          title="Response time unavailable"
          error={response.error}
          onRetry={() => void response.refetch()}
        />
      ) : !summary || summary.sample_count === 0 ? (
        <PageEmpty
          icon={<Timer className="size-5" />}
          title="No reply pairs in this window"
          body="Widen the range, or switch direction."
        />
      ) : (
        <div className="grid max-w-4xl gap-8">
          <FactList
            facts={[
              ["Median", formatDuration(summary.clock_p50_seconds)],
              ["90th percentile", formatDuration(summary.clock_p90_seconds)],
              ["Median, business hours", businessHours(summary.business_hours_p50_seconds)],
              ["90th, business hours", businessHours(summary.business_hours_p90_seconds)],
              ["Samples", plural(summary.sample_count, "reply", "replies")],
            ]}
          />
          {histogram.length > 0 ? (
            <div>
              <h3 className="mb-2 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
                Distribution
              </h3>
              <BarList
                label="Reply latency distribution"
                rows={histogram.map((bucket, index) => ({
                  id: String(bucket.upper_bound_seconds),
                  label: bucketLabel(
                    bucket.upper_bound_seconds,
                    histogram[index - 1]?.upper_bound_seconds ?? null,
                  ),
                  value: bucket.count,
                  display: plural(bucket.count, "reply", "replies"),
                }))}
              />
            </div>
          ) : null}
        </div>
      )}
    </PageSection>
  );
}

/** The daemon reports 0 (or null) until the business-hours backfill has run. */
function businessHours(seconds: number | null | undefined): string {
  return seconds ? formatDuration(seconds) : "not computed yet";
}
