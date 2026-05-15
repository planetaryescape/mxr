import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pause, Play, RefreshCw } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";

import {
  fetchActivityCount,
  fetchActivityList,
  formatTimestamp,
  pauseActivity,
  redactActivity,
  resumeActivity,
  type ActivityEntry,
  type ActivityTier,
  type ClientKind,
} from "./api";
import { Button } from "@/components/ui/button";

const WINDOWS: { label: string; ms: number | null }[] = [
  { label: "1h", ms: 3_600_000 },
  { label: "24h", ms: 86_400_000 },
  { label: "7d", ms: 7 * 86_400_000 },
  { label: "30d", ms: 30 * 86_400_000 },
  { label: "All", ms: null },
];

const SOURCES: ClientKind[] = ["tui", "cli", "web", "daemon"];
const TIERS: ActivityTier[] = ["important", "standard", "ephemeral"];

interface ActivityBrowserProps {
  /** When true, omit the outer page header (the embedding surface owns it). */
  embedded?: boolean;
}

export function ActivityBrowser({ embedded = false }: ActivityBrowserProps) {
  const queryClient = useQueryClient();
  const [windowMs, setWindowMs] = useState<number | null>(86_400_000);
  const [sources, setSources] = useState<Set<ClientKind>>(new Set());
  const [tiers, setTiers] = useState<Set<ActivityTier>>(new Set());
  const [prefix, setPrefix] = useState<string>("");
  const [query, setQuery] = useState<string>("");
  const [includeRedacted, setIncludeRedacted] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  const filterParams = useMemo(
    () => ({
      since: windowMs !== null ? Date.now() - windowMs : undefined,
      source: sources.size ? Array.from(sources) : undefined,
      tier: tiers.size ? Array.from(tiers) : undefined,
      prefix: prefix.trim() || undefined,
      query: query.trim() || undefined,
      include_redacted: includeRedacted || undefined,
      limit: 200,
    }),
    [windowMs, sources, tiers, prefix, query, includeRedacted],
  );

  const list = useQuery({
    queryKey: ["activity", "list", filterParams],
    queryFn: () => fetchActivityList(filterParams),
    refetchOnWindowFocus: false,
  });

  const count = useQuery({
    queryKey: ["activity", "count", filterParams],
    queryFn: () => fetchActivityCount(filterParams),
    refetchOnWindowFocus: false,
  });

  const pauseMut = useMutation({
    mutationFn: () => pauseActivity(null),
    onSuccess: () => {
      toast.success("Activity recording paused");
      queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
    onError: (e: Error) => toast.error(e.message),
  });

  const resumeMut = useMutation({
    mutationFn: () => resumeActivity(),
    onSuccess: () => {
      toast.success("Activity recording resumed");
      queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
    onError: (e: Error) => toast.error(e.message),
  });

  const redactMut = useMutation({
    mutationFn: (ids: number[]) => redactActivity(ids, null, false),
    onSuccess: (data) => {
      toast.success(`Tombstoned ${data?.count ?? "?"} rows`);
      setSelectedIds(new Set());
      queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
    onError: (e: Error) => toast.error(e.message),
  });

  function toggle<T>(set: Set<T>, item: T): Set<T> {
    const next = new Set(set);
    if (next.has(item)) next.delete(item);
    else next.add(item);
    return next;
  }

  const entries: ActivityEntry[] = list.data?.entries ?? [];

  return (
    <div className="flex h-full flex-col">
      {!embedded && (
        <header className="flex items-center justify-between border-b px-4 py-3">
          <div className="flex items-center gap-3">
            <h1 className="text-lg font-semibold">Activity</h1>
            <span className="text-sm text-muted-foreground">
              {count.data?.count ?? "—"} rows in window
            </span>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => queryClient.invalidateQueries({ queryKey: ["activity"] })}
            >
              <RefreshCw className="h-4 w-4" />
            </Button>
            <Button variant="outline" size="sm" onClick={() => pauseMut.mutate()}>
              <Pause className="mr-1 h-4 w-4" />
              Pause
            </Button>
            <Button variant="outline" size="sm" onClick={() => resumeMut.mutate()}>
              <Play className="mr-1 h-4 w-4" />
              Resume
            </Button>
          </div>
        </header>
      )}

      {embedded && (
        <div className="mb-3 flex items-center justify-between">
          <p className="text-2xs text-muted-foreground">
            {count.data?.count ?? "—"} rows in window · local-only, never transmitted
          </p>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => queryClient.invalidateQueries({ queryKey: ["activity"] })}
            >
              <RefreshCw className="size-3" />
            </Button>
            <Button variant="outline" size="sm" onClick={() => pauseMut.mutate()}>
              <Pause className="mr-1 size-3" />
              Pause
            </Button>
            <Button variant="outline" size="sm" onClick={() => resumeMut.mutate()}>
              <Play className="mr-1 size-3" />
              Resume
            </Button>
          </div>
        </div>
      )}

      <div className="grid grid-cols-[240px_1fr] gap-4 px-4 py-3">
        <aside className="space-y-4">
          <FilterGroup label="Window">
            <div className="flex flex-wrap gap-1">
              {WINDOWS.map((w) => (
                <Button
                  key={w.label}
                  size="sm"
                  variant={windowMs === w.ms ? "default" : "outline"}
                  onClick={() => setWindowMs(w.ms)}
                >
                  {w.label}
                </Button>
              ))}
            </div>
          </FilterGroup>

          <FilterGroup label="Source">
            <div className="flex flex-wrap gap-1">
              {SOURCES.map((s) => (
                <Button
                  key={s}
                  size="sm"
                  variant={sources.has(s) ? "default" : "outline"}
                  onClick={() => setSources(toggle(sources, s))}
                >
                  {s}
                </Button>
              ))}
            </div>
          </FilterGroup>

          <FilterGroup label="Tier">
            <div className="flex flex-wrap gap-1">
              {TIERS.map((t) => (
                <Button
                  key={t}
                  size="sm"
                  variant={tiers.has(t) ? "default" : "outline"}
                  onClick={() => setTiers(toggle(tiers, t))}
                >
                  {t}
                </Button>
              ))}
            </div>
          </FilterGroup>

          <FilterGroup label="Action prefix">
            <input
              type="text"
              value={prefix}
              onChange={(e) => setPrefix(e.target.value)}
              placeholder="mail."
              className="w-full rounded border border-border bg-background px-2 py-1 text-sm"
            />
          </FilterGroup>

          <FilterGroup label="Search (FTS5)">
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="invoice 2026"
              className="w-full rounded border border-border bg-background px-2 py-1 text-sm"
            />
          </FilterGroup>

          <FilterGroup label="">
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={includeRedacted}
                onChange={(e) => setIncludeRedacted(e.target.checked)}
              />
              Include redacted
            </label>
          </FilterGroup>
        </aside>

        <main className="min-w-0">
          {selectedIds.size > 0 && (
            <div className="mb-2 flex items-center justify-between rounded border border-border bg-muted/30 px-3 py-2">
              <span className="text-sm">{selectedIds.size} selected</span>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setSelectedIds(new Set())}
                >
                  Clear
                </Button>
                <Button
                  size="sm"
                  variant="destructive"
                  onClick={() => {
                    if (
                      window.confirm(
                        `Tombstone ${selectedIds.size} activity rows? This is irreversible.`,
                      )
                    ) {
                      redactMut.mutate(Array.from(selectedIds));
                    }
                  }}
                >
                  Redact selected
                </Button>
              </div>
            </div>
          )}

          {list.isLoading && <p className="text-sm text-muted-foreground">Loading…</p>}
          {list.error && <p className="text-sm text-destructive">{(list.error as Error).message}</p>}
          {!list.isLoading && entries.length === 0 && (
            <p className="text-sm text-muted-foreground">
              No activity in this window. Try a wider time range, or wait — mxr starts recording as
              you use it.
            </p>
          )}

          {entries.length > 0 && (
            <div className="overflow-x-auto rounded border border-border">
              <table className="w-full text-sm">
                <thead className="border-b border-border bg-muted/50">
                  <tr>
                    <th className="w-10 px-2 py-1.5 text-left">
                      <input
                        type="checkbox"
                        checked={
                          entries.length > 0 && selectedIds.size === entries.length
                        }
                        onChange={(e) => {
                          if (e.target.checked) {
                            setSelectedIds(new Set(entries.map((entry) => entry.id)));
                          } else {
                            setSelectedIds(new Set());
                          }
                        }}
                      />
                    </th>
                    <th className="px-2 py-1.5 text-left">Time</th>
                    <th className="px-2 py-1.5 text-left">Source</th>
                    <th className="px-2 py-1.5 text-left">Action</th>
                    <th className="px-2 py-1.5 text-left">Target</th>
                    <th className="px-2 py-1.5 text-left">Tier</th>
                    <th className="px-2 py-1.5 text-left">Context</th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((entry) => (
                    <tr
                      key={entry.id}
                      className={
                        entry.redacted
                          ? "border-b border-border opacity-50"
                          : "border-b border-border hover:bg-muted/30"
                      }
                    >
                      <td className="px-2 py-1.5">
                        <input
                          type="checkbox"
                          checked={selectedIds.has(entry.id)}
                          onChange={() => setSelectedIds(toggle(selectedIds, entry.id))}
                        />
                      </td>
                      <td className="whitespace-nowrap px-2 py-1.5 font-mono text-xs">
                        {formatTimestamp(entry.ts)}
                      </td>
                      <td className="px-2 py-1.5">
                        <SourceBadge source={entry.source} />
                      </td>
                      <td className="px-2 py-1.5 font-mono text-xs">{entry.action}</td>
                      <td className="px-2 py-1.5 font-mono text-xs">
                        {entry.target_kind && entry.target_id
                          ? `${entry.target_kind}:${entry.target_id.slice(0, 12)}`
                          : entry.target_kind ?? "—"}
                      </td>
                      <td className="px-2 py-1.5 text-xs">{entry.tier}</td>
                      <td className="max-w-[400px] truncate px-2 py-1.5 font-mono text-xs">
                        {entry.redacted
                          ? "(redacted)"
                          : entry.context
                            ? JSON.stringify(entry.context).slice(0, 120)
                            : ""}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}

export function ActivityRoute() {
  return <ActivityBrowser embedded={false} />;
}

function FilterGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      {label && <div className="text-xs font-medium text-muted-foreground">{label}</div>}
      {children}
    </div>
  );
}

function SourceBadge({ source }: { source: ClientKind }) {
  const color = {
    tui: "bg-cyan-100 text-cyan-900 dark:bg-cyan-900/30 dark:text-cyan-200",
    cli: "bg-fuchsia-100 text-fuchsia-900 dark:bg-fuchsia-900/30 dark:text-fuchsia-200",
    web: "bg-blue-100 text-blue-900 dark:bg-blue-900/30 dark:text-blue-200",
    daemon: "bg-muted text-muted-foreground",
  }[source];
  return (
    <span className={`inline-block rounded px-1.5 py-0.5 text-xs font-medium ${color}`}>{source}</span>
  );
}
