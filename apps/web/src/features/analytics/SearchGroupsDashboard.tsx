import { useQuery } from "@tanstack/react-query";
import { Search } from "lucide-react";
import { useState, type FormEvent } from "react";

import { BarList, Segmented, searchValue, useDrillToSearch } from "./analyticsParts";
import { PageSection } from "@/components/Page";
import { PageEmpty, PageError, PageNote, PageSkeleton } from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { fetchSearchGroups, searchGroupsKey, type SearchGroupBy } from "@/features/search/api";
import { plural } from "@/lib/format";
import { useUiPrefs } from "@/state/uiPrefsStore";

const GROUPS: { id: SearchGroupBy; label: string }[] = [
  { id: "from", label: "Sender" },
  { id: "list", label: "List" },
  { id: "category", label: "Category" },
];

/** The TUI's drill: the original query narrowed by the group's operator. */
function drillQuery(query: string, groupBy: SearchGroupBy, key: string): string {
  return `${query} ${groupBy}:${searchValue(key)}`.trim();
}

/** TUI "Search groups": aggregate any query by sender, list or category. */
export function SearchGroupsDashboard() {
  const drill = useDrillToSearch();
  const account = useUiPrefs((state) => state.accountScope) ?? undefined;
  const [draft, setDraft] = useState("is:unread");
  const [query, setQuery] = useState("is:unread");
  const [groupBy, setGroupBy] = useState<SearchGroupBy>("from");
  const params = { q: query, groupBy, account, limit: 50 };
  const groups = useQuery({
    queryKey: searchGroupsKey(params),
    queryFn: ({ signal }) => fetchSearchGroups(params, { signal }),
    enabled: query.trim().length > 0,
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setQuery(draft.trim());
  };

  const rows = groups.data?.groups ?? [];
  return (
    <PageSection
      title="Search groups"
      description={
        groups.data
          ? `${plural(groups.data.total, "message")} in ${plural(rows.length, "group")}`
          : undefined
      }
      actions={
        <Segmented
          value={groupBy}
          options={GROUPS}
          onChange={setGroupBy}
          label="Group results by"
        />
      }
    >
      <form onSubmit={submit} className="mb-3 flex max-w-xl gap-2">
        <Input
          aria-label="Search query to group"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          placeholder="older_than:1y has:attachment"
          className="h-8 flex-1 font-mono text-xs"
        />
        <Button type="submit" size="sm" disabled={!draft.trim()}>
          Group
        </Button>
      </form>
      <PageNote>Any search query works. Select a group to open its messages as a search.</PageNote>
      {!query ? (
        <PageEmpty icon={<Search className="size-5" />} title="Type a query to group" />
      ) : groups.isPending ? (
        <PageSkeleton label="Grouping results" />
      ) : groups.isError ? (
        <PageError
          title="Grouping failed"
          error={groups.error}
          onRetry={() => void groups.refetch()}
        />
      ) : rows.length === 0 ? (
        <PageEmpty
          icon={<Search className="size-5" />}
          title="No matches"
          body="Try a broader query, or group by another field."
        />
      ) : (
        <BarList
          label="Search groups"
          rows={rows.map((row) => ({
            id: row.key,
            label: row.label || row.key,
            value: row.count,
            display: plural(row.count, "message"),
            meta: row.unread > 0 ? `${row.unread} unread` : undefined,
            onSelect: () => drill(drillQuery(query, groupBy, row.key)),
          }))}
        />
      )}
    </PageSection>
  );
}
