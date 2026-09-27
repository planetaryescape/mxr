import { useQuery } from "@tanstack/react-query";
import { HardDrive } from "lucide-react";
import { useState } from "react";

import {
  fetchLargestMessages,
  fetchStorageBreakdown,
  rangeDays,
  type AnalyticsRange,
  type StorageGroupBy,
} from "./api";
import { BarList, Segmented, searchValue, useDrillToSearch } from "./analyticsParts";
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
import { formatBytes } from "@/lib/utils";

const GROUPS: { id: StorageGroupBy; label: string }[] = [
  { id: "sender", label: "Sender" },
  { id: "mimetype", label: "MIME type" },
  { id: "label", label: "Label" },
];

/** TUI drill-down: sender and label map to operators; MIME types are not indexed. */
function drillQuery(groupBy: StorageGroupBy, key: string): string {
  if (groupBy === "sender") return `from:${searchValue(key)}`;
  if (groupBy === "label") return `label:${searchValue(key)}`;
  return "has:attachment";
}

export function StorageDashboard({ range }: { range: AnalyticsRange }) {
  const drill = useDrillToSearch();
  const [groupBy, setGroupBy] = useState<StorageGroupBy>("sender");
  const [keyword, setKeyword] = useState("");
  const breakdown = useQuery({
    queryKey: ["analytics", "storage", groupBy],
    queryFn: () => fetchStorageBreakdown(groupBy, 50),
  });
  const largest = useQuery({
    queryKey: ["analytics", "largest", range],
    queryFn: () => fetchLargestMessages(25, rangeDays(range)),
  });

  const needle = keyword.trim().toLowerCase();
  const rows = (breakdown.data?.rows ?? []).filter((row) => row.key.toLowerCase().includes(needle));

  return (
    <div className="grid gap-x-10 xl:grid-cols-[minmax(0,1fr)_minmax(0,24rem)]">
      <PageSection
        title={`Storage by ${GROUPS.find((group) => group.id === groupBy)?.label.toLowerCase()}`}
        actions={
          <Segmented
            value={groupBy}
            options={GROUPS}
            onChange={setGroupBy}
            label="Group storage by"
          />
        }
      >
        <PageNote>
          Local message and attachment weight, all time. Select a row to search that{" "}
          {groupBy === "mimetype"
            ? "slice (MIME types are not indexed, so it opens all attachments)"
            : groupBy}
          .
        </PageNote>
        <Input
          aria-label="Filter storage rows"
          value={keyword}
          onChange={(event) => setKeyword(event.target.value)}
          placeholder="Filter rows"
          className="mb-3 h-8 max-w-xs text-xs"
        />
        {breakdown.isPending ? (
          <PageSkeleton label="Loading storage" />
        ) : breakdown.isError ? (
          <PageError
            title="Storage unavailable"
            error={breakdown.error}
            onRetry={() => void breakdown.refetch()}
          />
        ) : rows.length === 0 ? (
          <PageEmpty
            icon={<HardDrive className="size-5" />}
            title={needle ? "No rows match the filter" : "No storage data yet"}
            body={
              needle ? "Clear the filter to see every row." : "Sync mail, then rebuild analytics."
            }
          />
        ) : (
          <BarList
            label="Storage rows"
            rows={rows.map((row) => ({
              id: row.key,
              label: row.key,
              value: row.bytes,
              display: formatBytes(row.bytes),
              meta: plural(row.count, "message"),
              onSelect: () => drill(drillQuery(groupBy, row.key)),
            }))}
          />
        )}
      </PageSection>
      <PageSection title="Largest messages" description="Opens the sender's mail, as in the TUI.">
        {largest.isPending ? (
          <PageSkeleton label="Loading largest messages" />
        ) : largest.isError ? (
          <PageError
            title="Largest messages unavailable"
            error={largest.error}
            onRetry={() => void largest.refetch()}
          />
        ) : largest.data.rows.length === 0 ? (
          <PageEmpty title="Nothing in this window" body="Widen the range to look further back." />
        ) : (
          <RuledList label="Largest messages">
            {largest.data.rows.map((row) => (
              <RuledRow
                key={row.message_id}
                title={row.subject.trim() || "(no subject)"}
                meta={`${row.from_email} · ${formatListDate(row.date)}`}
                aside={formatBytes(row.size_bytes)}
                onOpen={() => drill(`from:${row.from_email}`)}
                openLabel={`Search mail from ${row.from_email}`}
              />
            ))}
          </RuledList>
        )}
      </PageSection>
    </div>
  );
}
