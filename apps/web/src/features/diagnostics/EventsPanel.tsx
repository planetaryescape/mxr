import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, RefreshCw } from "lucide-react";
import { useMemo, useState } from "react";

import { fetchEventCategories, fetchEventCount, fetchEvents, type EventLogEntry } from "./api";
import { PageError, PageSkeleton } from "@/components/PageParts";
import { Button } from "@/components/ui/button";

const LEVELS = ["all", "trace", "debug", "info", "warn", "error"] as const;
const WINDOWS = [
  { label: "1h", secs: 3600 },
  { label: "24h", secs: 86_400 },
  { label: "7d", secs: 7 * 86_400 },
  { label: "30d", secs: 30 * 86_400 },
  { label: "All", secs: null },
] as const;

function levelClasses(level: string): string {
  switch (level.toLowerCase()) {
    case "error":
      return "bg-destructive/10 text-destructive";
    case "warn":
      return "bg-warning/10 text-warning";
    case "info":
      return "bg-primary-muted text-primary";
    case "debug":
      return "bg-success/10 text-success";
    default:
      return "bg-muted text-muted-foreground";
  }
}

function eventEntryKey(entry: EventLogEntry): string {
  return [
    entry.timestamp,
    entry.level,
    entry.category,
    entry.account_id ?? "",
    entry.message_id ?? "",
    entry.rule_id ?? "",
    entry.summary,
  ].join("|");
}

export function EventsPanel() {
  const queryClient = useQueryClient();
  const [level, setLevel] = useState("all");
  const [category, setCategory] = useState("all");
  const [search, setSearch] = useState("");
  const [windowSecs, setWindowSecs] = useState<number | null>(86_400);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(50);
  const [expanded, setExpanded] = useState<string | null>(null);

  const baseFilter = useMemo(
    () => ({
      level: level === "all" ? undefined : level,
      category: category === "all" ? undefined : category,
      since: windowSecs !== null ? Math.floor(Date.now() / 1000) - windowSecs : undefined,
      search: search.trim() || undefined,
    }),
    [level, category, search, windowSecs],
  );

  const events = useQuery({
    queryKey: [
      "diagnostics",
      "events",
      { ...baseFilter, limit: pageSize, offset: page * pageSize },
    ],
    queryFn: () =>
      fetchEvents({
        ...baseFilter,
        limit: pageSize,
        offset: page * pageSize,
      }),
    refetchOnWindowFocus: false,
  });

  const count = useQuery({
    queryKey: ["diagnostics", "events", "count", baseFilter],
    queryFn: () => fetchEventCount(baseFilter),
    refetchOnWindowFocus: false,
  });

  const categories = useQuery({
    queryKey: ["diagnostics", "events", "categories"],
    queryFn: fetchEventCategories,
    staleTime: 60_000,
  });

  const entries: EventLogEntry[] = events.data?.entries ?? [];
  const total = count.data?.count ?? 0;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));

  function applyFilter(update: () => void) {
    setPage(0);
    update();
  }

  return (
    <section>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
            Event log
          </h2>
          <p className="text-2xs text-muted-foreground">
            Showing {entries.length} of {total} · page {page + 1} of {totalPages}
          </p>
        </div>
        <Button
          variant="ghost"
          size="sm"
          aria-label="Refresh events"
          onClick={() => {
            void queryClient.invalidateQueries({ queryKey: ["diagnostics", "events"] });
          }}
        >
          <RefreshCw className="size-3" />
        </Button>
      </div>

      <div className="mb-3 grid gap-2 md:grid-cols-[1fr_auto_auto_auto]">
        <input
          type="text"
          value={search}
          onChange={(e) => applyFilter(() => setSearch(e.target.value))}
          placeholder="Search summary + details"
          className="w-full h-8 rounded-md border border-border bg-background px-2 text-2xs"
        />
        <select
          value={level}
          onChange={(e) => applyFilter(() => setLevel(e.target.value))}
          className="h-8 rounded-md border border-border bg-background px-2 text-2xs"
        >
          {LEVELS.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
        <select
          value={category}
          onChange={(e) => applyFilter(() => setCategory(e.target.value))}
          className="h-8 rounded-md border border-border bg-background px-2 text-2xs"
        >
          <option value="all">all categories</option>
          {(categories.data?.categories ?? []).map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <div className="flex gap-1">
          {WINDOWS.map((w) => (
            <Button
              key={w.label}
              variant={windowSecs === w.secs ? "default" : "outline"}
              size="sm"
              onClick={() => applyFilter(() => setWindowSecs(w.secs))}
            >
              {w.label}
            </Button>
          ))}
        </div>
      </div>

      {events.isPending && <PageSkeleton rows={8} label="Loading events" />}
      {events.isError && (
        <PageError
          title="Events unavailable"
          error={events.error}
          onRetry={() => void events.refetch()}
        />
      )}

      {events.isSuccess && entries.length === 0 && (
        <div className="border-y border-border/60 py-6 text-center text-[13px] text-muted-foreground">
          No events match the current filters. Try a wider window or clear the search.
        </div>
      )}

      {entries.length > 0 && (
        <div className="overflow-x-auto border-y border-border/60">
          <table className="w-full text-2xs">
            <thead className="bg-muted/50">
              <tr>
                <th className="w-44 px-2 py-1.5 text-left">Timestamp</th>
                <th className="w-16 px-2 py-1.5 text-left">Level</th>
                <th className="w-24 px-2 py-1.5 text-left">Category</th>
                <th className="px-2 py-1.5 text-left">Summary</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry, index) => {
                // Identical events in the same second exist (repeated sync), so the
                // row position disambiguates.
                const key = `${eventEntryKey(entry)}|${page * pageSize + index}`;
                const isOpen = expanded === key;
                return (
                  <tr
                    key={key}
                    className="cursor-pointer border-b border-border/40 last:border-0 hover:bg-muted/30"
                    onClick={() => setExpanded(isOpen ? null : key)}
                  >
                    <td className="whitespace-nowrap px-2 py-1.5 font-mono">
                      {new Date(entry.timestamp * 1000).toLocaleString()}
                    </td>
                    <td className="px-2 py-1.5">
                      <span
                        className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase ${levelClasses(entry.level)}`}
                      >
                        {entry.level}
                      </span>
                    </td>
                    <td className="px-2 py-1.5 font-mono text-foreground">{entry.category}</td>
                    <td className="px-2 py-1.5">
                      <div className="text-foreground">{entry.summary}</div>
                      {isOpen && (
                        <div className="mt-1 space-y-0.5 text-muted-foreground">
                          {entry.account_id && (
                            <div>
                              <span className="text-muted-foreground/70">account: </span>
                              <code>{entry.account_id}</code>
                            </div>
                          )}
                          {entry.message_id && (
                            <div>
                              <span className="text-muted-foreground/70">message: </span>
                              <code>{entry.message_id}</code>
                            </div>
                          )}
                          {entry.rule_id && (
                            <div>
                              <span className="text-muted-foreground/70">rule: </span>
                              <code>{entry.rule_id}</code>
                            </div>
                          )}
                          {entry.details && (
                            <pre className="mt-1 whitespace-pre-wrap break-all rounded bg-background/50 p-2 text-foreground">
                              {entry.details}
                            </pre>
                          )}
                        </div>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      <div className="mt-3 flex items-center justify-between gap-2 text-2xs">
        <select
          value={pageSize}
          onChange={(e) => applyFilter(() => setPageSize(Number(e.target.value)))}
          className="h-8 rounded-md border border-border bg-background px-2"
        >
          {[25, 50, 100, 200, 500].map((n) => (
            <option key={n} value={n}>
              {n} per page
            </option>
          ))}
        </select>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPage((p) => Math.max(0, p - 1))}
            disabled={page === 0}
          >
            <ChevronLeft className="size-3" /> Prev
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
            disabled={page + 1 >= totalPages}
          >
            Next <ChevronRight className="size-3" />
          </Button>
        </div>
      </div>
    </section>
  );
}
