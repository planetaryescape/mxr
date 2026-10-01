import { useQuery } from "@tanstack/react-query";
import { Hourglass } from "lucide-react";
import { useState } from "react";

import { fetchStaleThreads, rangeDays, type AnalyticsRange } from "./api";
import { Segmented, useOpenThread } from "./analyticsParts";
import { PageSection } from "@/components/Page";
import {
  PageEmpty,
  PageError,
  PageNote,
  PageSkeleton,
  RuledList,
  RuledRow,
} from "@/components/PageParts";
import { formatWhen, plural } from "@/lib/format";

type Perspective = "mine" | "theirs";

const PERSPECTIVES: { id: Perspective; label: string }[] = [
  { id: "mine", label: "I owe" },
  { id: "theirs", label: "They owe" },
];

export function StaleDashboard({ range }: { range: AnalyticsRange }) {
  const openThread = useOpenThread();
  const [perspective, setPerspective] = useState<Perspective>("mine");
  const stale = useQuery({
    queryKey: ["analytics", "stale", perspective, range],
    queryFn: () => fetchStaleThreads(perspective, 14, rangeDays(range)),
  });

  return (
    <PageSection
      title="Stale threads"
      description={stale.data ? plural(stale.data.rows.length, "thread") : undefined}
      actions={
        <Segmented
          value={perspective}
          options={PERSPECTIVES}
          onChange={setPerspective}
          label="Whose reply is overdue"
        />
      }
    >
      <PageNote>
        Conversations silent for more than 14 days where{" "}
        {perspective === "mine" ? "the last message came to you" : "you sent the last message"}.
      </PageNote>
      {stale.isPending ? (
        <PageSkeleton label="Loading stale threads" />
      ) : stale.isError ? (
        <PageError
          title="Stale threads unavailable"
          error={stale.error}
          onRetry={() => void stale.refetch()}
        />
      ) : stale.data.rows.length === 0 ? (
        <PageEmpty
          icon={<Hourglass className="size-5" />}
          title="Nothing stale in this window"
          body="Every conversation in range has moved in the last two weeks."
        />
      ) : (
        <RuledList label="Stale threads">
          {stale.data.rows.map((row) => (
            <RuledRow
              key={row.thread_id}
              title={row.latest_subject.trim() || "(no subject)"}
              meta={`${row.counterparty_email} · last ${formatWhen(row.latest_date)}`}
              aside={plural(row.days_stale, "day")}
              onOpen={() => openThread(row.thread_id)}
              openLabel={`Open ${row.latest_subject || "conversation"}`}
            />
          ))}
        </RuledList>
      )}
    </PageSection>
  );
}
