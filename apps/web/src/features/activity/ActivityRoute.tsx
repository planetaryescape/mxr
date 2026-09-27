import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { History, Pause, Play, RefreshCw } from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";
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
import { Page } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton } from "@/components/PageParts";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";

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
  /** When true, the embedding surface (diagnostics) owns the page header. */
  embedded?: boolean;
}

function toggle<T>(set: Set<T>, item: T): Set<T> {
  const next = new Set(set);
  if (next.has(item)) next.delete(item);
  else next.add(item);
  return next;
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
  const [confirmRedact, setConfirmRedact] = useState(false);

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
      void queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
    onError: (error: Error) =>
      toast.error("Could not pause recording", { description: error.message }),
  });
  const resumeMut = useMutation({
    mutationFn: () => resumeActivity(),
    onSuccess: () => {
      toast.success("Activity recording resumed");
      void queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
    onError: (error: Error) =>
      toast.error("Could not resume recording", { description: error.message }),
  });
  const redactMut = useMutation({
    mutationFn: (ids: number[]) => redactActivity(ids, null, false),
    onSuccess: (data, ids) => {
      toast.success(`Redacted ${plural(data?.count ?? ids.length, "entry", "entries")}`);
      setSelectedIds(new Set());
      setConfirmRedact(false);
      void queryClient.invalidateQueries({ queryKey: ["activity"] });
    },
    onError: (error: Error) => toast.error("Redaction failed", { description: error.message }),
  });

  const entries: ActivityEntry[] = list.data?.entries ?? [];

  const controls = (
    <>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="Refresh activity"
        onClick={() => void queryClient.invalidateQueries({ queryKey: ["activity"] })}
      >
        <RefreshCw className="size-3.5" />
      </Button>
      <Button
        variant="outline"
        size="sm"
        onClick={() => pauseMut.mutate()}
        disabled={pauseMut.isPending}
      >
        <Pause className="size-3" />
        Pause recording
      </Button>
      <Button
        variant="outline"
        size="sm"
        onClick={() => resumeMut.mutate()}
        disabled={resumeMut.isPending}
      >
        <Play className="size-3" />
        Resume
      </Button>
    </>
  );

  const body = (
    <div className="grid gap-6 md:grid-cols-[12rem_minmax(0,1fr)]">
      <aside className="space-y-5" aria-label="Activity filters">
        <FilterGroup label="Window">
          {WINDOWS.map((w) => (
            <Chip key={w.label} active={windowMs === w.ms} onClick={() => setWindowMs(w.ms)}>
              {w.label}
            </Chip>
          ))}
        </FilterGroup>
        <FilterGroup label="Source">
          {SOURCES.map((s) => (
            <Chip key={s} active={sources.has(s)} onClick={() => setSources(toggle(sources, s))}>
              {s}
            </Chip>
          ))}
        </FilterGroup>
        <FilterGroup label="Tier">
          {TIERS.map((t) => (
            <Chip key={t} active={tiers.has(t)} onClick={() => setTiers(toggle(tiers, t))}>
              {t}
            </Chip>
          ))}
        </FilterGroup>
        <div className="space-y-1">
          <Label htmlFor="activity-prefix" className="text-xs">
            Action prefix
          </Label>
          <Input
            id="activity-prefix"
            value={prefix}
            onChange={(e) => setPrefix(e.target.value)}
            placeholder="mail."
            className="h-8 font-mono text-xs"
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor="activity-search" className="text-xs">
            Search
          </Label>
          <Input
            id="activity-search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="invoice 2026"
            className="h-8 text-xs"
          />
        </div>
        <div className="flex items-center gap-2">
          <Checkbox
            id="activity-redacted"
            checked={includeRedacted}
            onCheckedChange={(checked) => setIncludeRedacted(checked === true)}
          />
          <Label htmlFor="activity-redacted" className="text-xs font-normal">
            Include redacted
          </Label>
        </div>
      </aside>

      <div className="min-w-0">
        <div className="mb-2 flex min-h-8 items-center justify-between gap-3">
          <span className="font-mono text-2xs text-muted-foreground">
            {selectedIds.size > 0
              ? `${selectedIds.size} selected`
              : count.data
                ? `${plural(count.data.count, "entry", "entries")} in window`
                : ""}
          </span>
          {selectedIds.size > 0 ? (
            <div className="flex gap-2">
              <Button size="sm" variant="ghost" onClick={() => setSelectedIds(new Set())}>
                Clear selection
              </Button>
              <Button size="sm" variant="destructive" onClick={() => setConfirmRedact(true)}>
                Redact selected
              </Button>
            </div>
          ) : null}
        </div>

        {list.isPending ? (
          <PageSkeleton rows={8} label="Loading activity" />
        ) : list.isError ? (
          <PageError
            title="Activity unavailable"
            error={list.error}
            onRetry={() => void list.refetch()}
          />
        ) : entries.length === 0 ? (
          <PageEmpty
            icon={<History className="size-5" />}
            title="No activity in this window"
            body="Try a wider time range or fewer filters. mxr records actions as you use it."
          />
        ) : (
          <div className="overflow-x-auto border-y border-border/60">
            <table className="w-full text-[13px]">
              <thead className="border-b border-border/60 font-mono text-[10.5px] uppercase tracking-[0.08em] text-muted-foreground">
                <tr>
                  <th className="w-8 px-2 py-2 text-left">
                    <Checkbox
                      aria-label="Select all shown entries"
                      checked={selectedIds.size === entries.length}
                      onCheckedChange={(checked) =>
                        setSelectedIds(
                          checked === true ? new Set(entries.map((e) => e.id)) : new Set(),
                        )
                      }
                    />
                  </th>
                  <th className="px-2 py-2 text-left font-normal">Time</th>
                  <th className="px-2 py-2 text-left font-normal">Source</th>
                  <th className="px-2 py-2 text-left font-normal">Action</th>
                  <th className="px-2 py-2 text-left font-normal">Target</th>
                  <th className="px-2 py-2 text-left font-normal">Tier</th>
                  <th className="px-2 py-2 text-left font-normal">Context</th>
                </tr>
              </thead>
              <tbody>
                {entries.map((entry) => (
                  <tr
                    key={entry.id}
                    className={cn(
                      "border-b border-border/40 last:border-0",
                      entry.redacted ? "text-muted-foreground" : "hover:bg-muted/30",
                    )}
                  >
                    <td className="px-2 py-1.5">
                      <Checkbox
                        aria-label={`Select entry ${entry.id}`}
                        checked={selectedIds.has(entry.id)}
                        onCheckedChange={() => setSelectedIds(toggle(selectedIds, entry.id))}
                      />
                    </td>
                    <td className="whitespace-nowrap px-2 py-1.5 font-mono text-2xs tabular-nums">
                      {formatTimestamp(entry.ts)}
                    </td>
                    <td className="px-2 py-1.5 font-mono text-2xs text-primary">{entry.source}</td>
                    <td className="px-2 py-1.5 font-mono text-2xs">{entry.action}</td>
                    <td className="whitespace-nowrap px-2 py-1.5 font-mono text-2xs text-muted-foreground">
                      {entry.target_kind && entry.target_id
                        ? `${entry.target_kind}:${entry.target_id.slice(0, 12)}`
                        : (entry.target_kind ?? "")}
                    </td>
                    <td className="px-2 py-1.5 text-2xs">{entry.tier}</td>
                    <td className="max-w-[400px] truncate px-2 py-1.5 font-mono text-2xs text-muted-foreground">
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
      </div>

      <AlertDialog open={confirmRedact} onOpenChange={setConfirmRedact}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Redact {plural(selectedIds.size, "entry", "entries")}?
            </AlertDialogTitle>
            <AlertDialogDescription>
              Their details are replaced with a tombstone. This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={redactMut.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={redactMut.isPending}
              onClick={(event) => {
                event.preventDefault();
                redactMut.mutate(Array.from(selectedIds));
              }}
            >
              Redact
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );

  if (embedded)
    return (
      <div>
        <div className="mb-4 flex flex-wrap items-center justify-end gap-2">{controls}</div>
        {body}
      </div>
    );
  return (
    <Page
      title="Activity"
      description="A local record of what you did in the TUI, CLI and web. It never leaves this machine."
      width="full"
      actions={controls}
    >
      <div className="mx-auto max-w-[84rem]">{body}</div>
    </Page>
  );
}

export function ActivityRoute() {
  return <ActivityBrowser />;
}

function FilterGroup({ label, children }: { label: string; children: ReactNode }) {
  return (
    <fieldset>
      <legend className="mb-1.5 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
        {label}
      </legend>
      <div className="flex flex-wrap gap-1">{children}</div>
    </fieldset>
  );
}

function Chip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "h-6 rounded-md border px-2 font-mono text-2xs outline-none focus-visible:ring-2 focus-visible:ring-ring",
        active
          ? "border-primary/50 bg-primary-muted text-foreground"
          : "border-border text-muted-foreground hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}
