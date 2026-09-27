import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { fetchContactAsymmetry, fetchContactDecay } from "./api";
import { useDrillToSearch } from "./analyticsParts";
import { PageSection } from "@/components/Page";
import {
  PageEmpty,
  PageError,
  PageNote,
  PageSkeleton,
  RuledList,
  RuledRow,
} from "@/components/PageParts";
import { Input } from "@/components/ui/input";
import { formatListDate, plural } from "@/lib/format";

export function ContactsDashboard() {
  const drill = useDrillToSearch();
  const [thresholdDays, setThresholdDays] = useState(30);
  const asymmetry = useQuery({
    queryKey: ["analytics", "contacts", "asymmetry"],
    queryFn: () => fetchContactAsymmetry(),
  });
  const decay = useQuery({
    queryKey: ["analytics", "contacts", "decay", thresholdDays],
    queryFn: () => fetchContactDecay(40, thresholdDays),
  });

  return (
    <div className="grid gap-x-10 lg:grid-cols-2">
      <PageSection
        title="Asymmetry"
        description="Contacts who write far more than you reply, or the reverse."
      >
        {asymmetry.isPending ? (
          <PageSkeleton label="Loading asymmetry" />
        ) : asymmetry.isError ? (
          <PageError
            title="Asymmetry unavailable"
            error={asymmetry.error}
            onRetry={() => void asymmetry.refetch()}
          />
        ) : asymmetry.data.rows.length === 0 ? (
          <PageEmpty
            title="No contact data yet"
            body="Refresh contacts from the command palette after a sync."
          />
        ) : (
          <RuledList label="Asymmetric contacts">
            {asymmetry.data.rows.map((row) => (
              <RuledRow
                key={row.email}
                title={row.display_name || row.email}
                meta={`${row.email} · ${row.total_inbound} in, ${row.total_outbound} out`}
                aside={`${Math.round(row.asymmetry * 100)}%`}
                onOpen={() => drill(`from:${row.email}`)}
                openLabel={`Search mail from ${row.email}`}
              />
            ))}
          </RuledList>
        )}
      </PageSection>
      <PageSection
        title="Decay"
        description="Contacts who have gone quiet."
        actions={
          <label className="flex items-center gap-2 text-2xs text-muted-foreground">
            <Input
              aria-label="Quiet for at least, in days"
              type="number"
              min={1}
              value={thresholdDays}
              onChange={(event) => setThresholdDays(Math.max(1, Number(event.target.value) || 1))}
              className="h-7 w-16 text-xs"
            />
            days quiet
          </label>
        }
      >
        <PageNote>No inbound mail for at least {plural(thresholdDays, "day")}.</PageNote>
        {decay.isPending ? (
          <PageSkeleton label="Loading decay" />
        ) : decay.isError ? (
          <PageError
            title="Decay unavailable"
            error={decay.error}
            onRetry={() => void decay.refetch()}
          />
        ) : decay.data.rows.length === 0 ? (
          <PageEmpty
            title="Nobody has gone quiet"
            body="Lower the threshold to see contacts fading sooner."
          />
        ) : (
          <RuledList label="Quiet contacts">
            {decay.data.rows.map((row) => (
              <RuledRow
                key={row.email}
                title={row.display_name || row.email}
                meta={`${row.email} · last heard ${formatListDate(row.last_inbound_at)}`}
                aside={plural(row.days_since_inbound, "day")}
                onOpen={() => drill(`from:${row.email}`)}
                openLabel={`Search mail from ${row.email}`}
              />
            ))}
          </RuledList>
        )}
      </PageSection>
    </div>
  );
}
