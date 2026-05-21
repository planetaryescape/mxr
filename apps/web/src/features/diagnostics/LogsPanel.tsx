import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Pause, Play, RefreshCw } from "lucide-react";
import { useMemo, useState } from "react";

import { fetchLogs } from "./api";
import { Button } from "@/components/ui/button";

const LEVELS = ["all", "trace", "debug", "info", "warn", "error"] as const;
const LIMITS = [50, 200, 500, 1000];

interface LogRow {
  raw: string;
  timestamp?: string;
  level?: string;
  message?: string;
}

function parseLine(line: string): LogRow {
  // tracing-subscriber JSON file appender emits:
  //   {"timestamp":"…","level":"INFO","fields":{"message":"…"},…}
  if (line.startsWith("{")) {
    try {
      const obj = JSON.parse(line) as Record<string, unknown>;
      const timestamp = typeof obj.timestamp === "string" ? obj.timestamp : undefined;
      const level = typeof obj.level === "string" ? obj.level : undefined;
      const fields = obj.fields as Record<string, unknown> | undefined;
      const fieldMessage =
        fields && typeof fields.message === "string" ? fields.message : undefined;
      const topMessage = typeof obj.message === "string" ? obj.message : undefined;
      return { raw: line, timestamp, level, message: fieldMessage ?? topMessage ?? line };
    } catch {
      // fall through
    }
  }
  return { raw: line };
}

function levelClasses(level?: string): string {
  switch ((level ?? "").toLowerCase()) {
    case "error":
      return "bg-red-100 text-red-900 dark:bg-red-900/30 dark:text-red-200";
    case "warn":
      return "bg-amber-100 text-amber-900 dark:bg-amber-900/30 dark:text-amber-200";
    case "info":
      return "bg-blue-100 text-blue-900 dark:bg-blue-900/30 dark:text-blue-200";
    case "debug":
      return "bg-emerald-100 text-emerald-900 dark:bg-emerald-900/30 dark:text-emerald-200";
    case "trace":
      return "bg-muted text-muted-foreground";
    default:
      return "bg-muted text-muted-foreground";
  }
}

function logRowKey(row: LogRow): string {
  return [row.timestamp ?? "", row.level ?? "", row.message ?? "", row.raw].join("|");
}

export function LogsPanel() {
  const queryClient = useQueryClient();
  const [level, setLevel] = useState<string>("all");
  const [search, setSearch] = useState("");
  const [limit, setLimit] = useState<number>(200);
  const [follow, setFollow] = useState(true);

  const params = useMemo(
    () => ({
      limit,
      level: level === "all" ? undefined : level,
      search: search.trim() || undefined,
    }),
    [limit, level, search],
  );

  const logs = useQuery({
    queryKey: ["diagnostics", "logs", params],
    queryFn: () => fetchLogs(params),
    refetchInterval: follow ? 3_000 : false,
    refetchOnWindowFocus: false,
  });

  const rows = useMemo(() => {
    const lines = logs.data?.lines ?? [];
    return lines.map(parseLine);
  }, [logs.data]);

  return (
    <section className="rounded-xl border border-border bg-surface p-4">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-sm font-semibold">Daemon logs</h2>
          <p className="text-2xs text-muted-foreground">
            {rows.length} line{rows.length === 1 ? "" : "s"} · live tail every 3s
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search log lines"
            className="w-64 rounded border border-border bg-background px-2 py-1 text-2xs"
          />
          <select
            value={level}
            onChange={(e) => setLevel(e.target.value)}
            className="rounded border border-border bg-background px-2 py-1 text-2xs"
          >
            {LEVELS.map((l) => (
              <option key={l} value={l}>
                {l}
              </option>
            ))}
          </select>
          <select
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
            className="rounded border border-border bg-background px-2 py-1 text-2xs"
          >
            {LIMITS.map((n) => (
              <option key={n} value={n}>
                {n} rows
              </option>
            ))}
          </select>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setFollow((f) => !f)}
            title={follow ? "Pause auto-refresh" : "Resume auto-refresh"}
          >
            {follow ? <Pause className="size-3" /> : <Play className="size-3" />}
            {follow ? "Live" : "Paused"}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => queryClient.invalidateQueries({ queryKey: ["diagnostics", "logs"] })}
          >
            <RefreshCw className="size-3" />
          </Button>
        </div>
      </div>
      {logs.isError && (
        <p className="text-2xs text-destructive">{(logs.error as Error).message}</p>
      )}
      {!logs.isError && rows.length === 0 && (
        <div className="rounded-lg bg-muted p-3 text-2xs text-muted-foreground">
          No log lines match the current filters.
        </div>
      )}
      {rows.length > 0 && (
        <div className="max-h-[480px] overflow-auto rounded-lg bg-muted/50 font-mono text-2xs">
          <table className="w-full">
            <tbody>
              {rows.map((row) => (
                <tr
                  key={logRowKey(row)}
                  className="border-b border-border/30 align-top last:border-0 hover:bg-background/50"
                >
                  <td className="whitespace-nowrap px-2 py-1 text-muted-foreground">
                    {row.timestamp
                      ? new Date(row.timestamp).toLocaleString()
                      : ""}
                  </td>
                  <td className="px-2 py-1">
                    {row.level && (
                      <span
                        className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase ${levelClasses(row.level)}`}
                      >
                        {row.level}
                      </span>
                    )}
                  </td>
                  <td className="px-2 py-1 text-foreground">{row.message ?? row.raw}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
