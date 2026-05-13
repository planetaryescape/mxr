import { useMutation, useQuery } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { BarChart3, RefreshCw, ShieldAlert } from "lucide-react";
import { useState } from "react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { toast } from "sonner";

import {
  fetchContactAsymmetry,
  fetchContactDecay,
  fetchLargestMessages,
  fetchResponseTime,
  fetchStaleThreads,
  fetchStorageBreakdown,
  fetchSubscriptions,
  fetchWrapped,
  rebuildAnalytics,
  unsubscribeSubscription,
  type AnalyticsRange,
  type ResponseDirection,
  type StorageGroupBy,
  type SubscriptionSummary,
  type WrappedSummary,
} from "./api";
import { EmptyState } from "@/components/EmptyState";
import { Alert, AlertDescription } from "@/components/ui/alert";
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
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn, formatBytes } from "@/lib/utils";
import { useModals } from "@/state/modalStore";

const analyticsTabs = [
  { id: "storage", label: "Storage" },
  { id: "stale", label: "Stale Threads" },
  { id: "contacts", label: "Contacts" },
  { id: "response-time", label: "Response Time" },
  { id: "subscriptions", label: "Subscriptions" },
  { id: "wrapped", label: "Wrapped" },
] as const;

export function AnalyticsDashboardRoute() {
  const { dashboard } = useParams({ from: "/analytics/$dashboard" });
  const [range, setRange] = useState<AnalyticsRange>("90d");
  const rebuild = useMutation({
    mutationFn: rebuildAnalytics,
    onSuccess: () => toast.success("Analytics rebuild queued"),
  });

  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="border-b border-border px-6 py-4">
        <div className="flex flex-wrap items-center gap-3">
          <div className="min-w-0 flex-1">
            <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
              Analytics
            </div>
            <h1 className="text-xl font-semibold tracking-tight">{dashboardTitle(dashboard)}</h1>
          </div>
          <Select value={range} onValueChange={(value) => setRange(value as AnalyticsRange)}>
            <SelectTrigger className="w-28" aria-label="Analytics range">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="7d">7 days</SelectItem>
              <SelectItem value="30d">30 days</SelectItem>
              <SelectItem value="90d">90 days</SelectItem>
              <SelectItem value="1y">1 year</SelectItem>
            </SelectContent>
          </Select>
          <Button variant="outline" onClick={() => rebuild.mutate()} disabled={rebuild.isPending}>
            <RefreshCw className="size-3" />
            Rebuild
          </Button>
        </div>
        <nav
          role="tablist"
          aria-label="Analytics dashboards"
          className="mt-4 flex flex-wrap items-center gap-1 text-sm"
        >
          {analyticsTabs.map((tab, index) => {
            const active = dashboard === tab.id;
            return (
              <div key={tab.id} className="flex items-center gap-1">
                {index > 0 ? (
                  <span aria-hidden="true" className="text-muted-foreground/50">
                    |
                  </span>
                ) : null}
                <Link
                  to="/analytics/$dashboard"
                  params={{ dashboard: tab.id }}
                  role="tab"
                  aria-selected={active}
                  className={cn(
                    "rounded-sm px-2 py-1 transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
                    active
                      ? "bg-primary-muted font-medium text-primary"
                      : "text-muted-foreground hover:bg-surface-elevated hover:text-foreground",
                  )}
                >
                  {tab.label}
                </Link>
              </div>
            );
          })}
        </nav>
      </header>
      <main className="min-h-0 flex-1 overflow-auto p-6">
        <Dashboard dashboard={dashboard} range={range} />
      </main>
    </div>
  );
}

function Dashboard({ dashboard, range }: { dashboard: string; range: AnalyticsRange }) {
  switch (dashboard) {
    case "storage":
      return <StorageDashboard range={range} />;
    case "stale":
      return <StaleDashboard />;
    case "contacts":
      return <ContactsDashboard />;
    case "response-time":
      return <ResponseTimeDashboard range={range} />;
    case "subscriptions":
      return <SubscriptionsDashboard />;
    case "wrapped":
      return <WrappedDashboard range={range} />;
    default:
      return <EmptyState icon={BarChart3} title="Unknown dashboard" description={dashboard} />;
  }
}

function StorageDashboard({ range }: { range: AnalyticsRange }) {
  const [groupBy, setGroupBy] = useState<StorageGroupBy>("sender");
  const [keyword, setKeyword] = useState("");
  const breakdown = useQuery({
    queryKey: ["analytics", "storage", range, groupBy],
    queryFn: () => fetchStorageBreakdown(groupBy, 50),
  });
  const largest = useQuery({
    queryKey: ["analytics", "largest", range],
    queryFn: () => fetchLargestMessages(25, rangeDays(range)),
  });
  const openRail = useModals((state) => state.openRightRail);
  if (breakdown.isError || largest.isError)
    return <AnalyticsError error={breakdown.error ?? largest.error} />;
  const rows =
    breakdown.data?.rows.map((row) => ({
      name: row.label ?? row.key ?? row.value ?? "unknown",
      bytes: row.bytes ?? row.total_bytes ?? 0,
      count: row.count ?? 0,
    })) ?? [];
  const filteredRows = rows.filter((row) =>
    row.name.toLowerCase().includes(keyword.trim().toLowerCase()),
  );
  return (
    <div className="grid gap-4 xl:grid-cols-[1fr_420px]">
      <Panel title={`Storage by ${groupByLabel(groupBy)}`}>
        <Explainer>
          What you're seeing: local SQLite message-body and attachment weight grouped by the
          selected dimension. Use the keyword filter to narrow noisy senders or MIME buckets.
        </Explainer>
        <div className="mb-3 grid gap-2 sm:grid-cols-[180px_1fr]">
          <Select value={groupBy} onValueChange={(value) => setGroupBy(value as StorageGroupBy)}>
            <SelectTrigger aria-label="Storage group by">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="sender">Sender</SelectItem>
              <SelectItem value="mimetype">MIME type</SelectItem>
              <SelectItem value="label">Label</SelectItem>
            </SelectContent>
          </Select>
          <Input
            aria-label="Storage keyword filter"
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
            placeholder="Filter buckets"
          />
        </div>
        <Chart
          rows={filteredRows}
          onClick={(row) =>
            openRail("analytics-drilldown", {
              title: row.name,
              items: [`${formatBytes(row.bytes)}`, `${row.count} messages`],
            })
          }
        />
      </Panel>
      <Panel title="Largest messages">
        <DataList
          rows={(largest.data?.rows ?? []).map((row) => ({
            id: row.message_id ?? row.subject ?? "message",
            title: row.subject ?? "(no subject)",
            meta: `${row.sender ?? "unknown"} · ${formatBytes(row.size_bytes ?? 0)}`,
          }))}
        />
      </Panel>
    </div>
  );
}

function StaleDashboard() {
  const [perspective, setPerspective] = useState<"mine" | "theirs">("mine");
  const stale = useQuery({
    queryKey: ["analytics", "stale", perspective],
    queryFn: () => fetchStaleThreads(perspective),
  });
  if (stale.isError) return <AnalyticsError error={stale.error} />;
  return (
    <Panel title="Stale threads">
      <Explainer>
        What you're seeing: threads with a stale ball-in-court. Toggle whether mxr thinks you owe a
        reply or the other side does.
      </Explainer>
      <ToggleGroup
        className="mb-3"
        type="single"
        value={perspective}
        onValueChange={(value) => {
          if (value) setPerspective(value as "mine" | "theirs");
        }}
        aria-label="Stale perspective"
      >
        <ToggleGroupItem value="mine" size="sm">
          I owe
        </ToggleGroupItem>
        <ToggleGroupItem value="theirs" size="sm">
          They owe
        </ToggleGroupItem>
      </ToggleGroup>
      <DataList rows={(stale.data?.rows ?? []).map(staleRow)} />
    </Panel>
  );
}

function ContactsDashboard() {
  const [thresholdDays, setThresholdDays] = useState(30);
  const asymmetry = useQuery({
    queryKey: ["analytics", "contacts", "asymmetry"],
    queryFn: () => fetchContactAsymmetry(),
  });
  const decay = useQuery({
    queryKey: ["analytics", "contacts", "decay", thresholdDays],
    queryFn: () => fetchContactDecay(40, thresholdDays),
  });
  if (asymmetry.isError || decay.isError)
    return <AnalyticsError error={asymmetry.error ?? decay.error} />;
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Panel title="Asymmetry">
        <Explainer>
          What you're seeing: contacts with uneven inbound/outbound volume, useful for finding
          relationships that need follow-up or boundaries.
        </Explainer>
        <DataList rows={(asymmetry.data?.rows ?? []).map(contactRow)} />
      </Panel>
      <Panel title="Decay">
        <Explainer>
          What you're seeing: contacts that have gone quiet beyond your threshold.
        </Explainer>
        <div className="mb-3 flex items-center gap-2">
          <Input
            aria-label="Decay threshold days"
            type="number"
            min={1}
            value={thresholdDays}
            onChange={(event) => setThresholdDays(Math.max(1, Number(event.target.value) || 1))}
            className="w-24"
          />
          <span className="text-2xs text-muted-foreground">days since last seen</span>
        </div>
        <DataList rows={(decay.data?.rows ?? []).map(contactRow)} />
      </Panel>
    </div>
  );
}

function ResponseTimeDashboard({ range }: { range: AnalyticsRange }) {
  const [direction, setDirection] = useState<ResponseDirection>("they_replied");
  const response = useQuery({
    queryKey: ["analytics", "response", range, direction],
    queryFn: () => fetchResponseTime(rangeDays(range), direction),
  });
  if (response.isError) return <AnalyticsError error={response.error} />;
  const summary = response.data?.summary;
  return (
    <div className="grid gap-4">
      <Panel title="Response time">
        <Explainer>
          What you're seeing: reply latency pairs. Switch direction to compare how fast others
          answer you against how fast you answer them.
        </Explainer>
        <ToggleGroup
          className="mb-3"
          type="single"
          value={direction}
          onValueChange={(value) => {
            if (value) setDirection(value as ResponseDirection);
          }}
          aria-label="Response direction"
        >
          <ToggleGroupItem value="they_replied" size="sm">
            Inbound replies
          </ToggleGroupItem>
          <ToggleGroupItem value="i_replied" size="sm">
            Outbound replies
          </ToggleGroupItem>
        </ToggleGroup>
        <div className="mt-4 grid gap-4 md:grid-cols-3">
          <Stat label="p50" value={`${Math.round(summary?.p50_minutes ?? 0)}m`} />
          <Stat label="p90" value={`${Math.round(summary?.p90_minutes ?? 0)}m`} />
          <Stat label="samples" value={String(summary?.count ?? 0)} />
        </div>
      </Panel>
    </div>
  );
}

export function SubscriptionsDashboard() {
  const [sort, setSort] = useState<"low-open" | "volume" | "recent">("low-open");
  const [confirm, setConfirm] = useState<SubscriptionSummary | null>(null);
  const subscriptions = useQuery({
    queryKey: ["subscriptions"],
    queryFn: () => fetchSubscriptions(100),
  });
  const unsubscribe = useMutation({
    mutationFn: (messageId: string) => unsubscribeSubscription(messageId),
    onSuccess: () => {
      toast.success("Unsubscribe requested");
      setConfirm(null);
    },
    onError: (error) => toast.error("Unsubscribe failed", { description: error.message }),
  });
  if (subscriptions.isError) return <AnalyticsError error={subscriptions.error} />;
  const rows = sortSubscriptions(subscriptions.data?.subscriptions ?? [], sort);
  return (
    <>
      <Panel title="Newsletter ROI">
        <Explainer>
          What you're seeing: bulk senders ranked by low open-rate, high volume, or recency.
          Unsubscribe uses the latest message's list-unsubscribe metadata.
        </Explainer>
        <Select value={sort} onValueChange={(value) => setSort(value as typeof sort)}>
          <SelectTrigger className="mb-3 w-44" aria-label="Subscription sort">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="low-open">Low open-rate</SelectItem>
            <SelectItem value="volume">Volume</SelectItem>
            <SelectItem value="recent">Recent</SelectItem>
          </SelectContent>
        </Select>
        <div className="divide-y divide-border">
          {rows.map((row) => (
            <div key={row.sender_email} className="flex items-center gap-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="truncate text-xs font-medium">
                  {row.sender_name ?? row.sender_email}
                </div>
                <div className="truncate text-2xs text-muted-foreground">
                  {row.message_count} messages · {openRateLabel(row)} opened ·{" "}
                  {row.latest_subject ?? "latest unknown"}
                </div>
              </div>
              <Button
                variant="outline"
                size="sm"
                disabled={!row.latest_message_id}
                onClick={() => setConfirm(row)}
              >
                Unsubscribe
              </Button>
            </div>
          ))}
        </div>
      </Panel>
      <AlertDialog open={Boolean(confirm)} onOpenChange={(open) => !open && setConfirm(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Unsubscribe from {confirm?.sender_name ?? confirm?.sender_email}?
            </AlertDialogTitle>
            <AlertDialogDescription>
              mxr will use the latest message's unsubscribe method. This may open a provider flow or
              send the unsubscribe request through the daemon.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={unsubscribe.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={!confirm?.latest_message_id || unsubscribe.isPending}
              onClick={(event) => {
                event.preventDefault();
                if (confirm?.latest_message_id) unsubscribe.mutate(confirm.latest_message_id);
              }}
            >
              <ShieldAlert className="size-3" />
              Confirm unsubscribe
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

function WrappedDashboard({ range }: { range: AnalyticsRange }) {
  const wrapped = useQuery({
    queryKey: ["analytics", "wrapped", range],
    queryFn: () => fetchWrapped(range),
  });
  if (wrapped.isError) return <AnalyticsError error={wrapped.error} />;
  const summary = wrapped.data?.summary;
  const totalMessages =
    (summary?.volume?.inbound_count ?? 0) + (summary?.volume?.outbound_count ?? 0);
  return (
    <div className="grid gap-4 lg:grid-cols-[1fr_1fr]">
      <section className="relative overflow-hidden rounded-2xl border border-border bg-[radial-gradient(circle_at_top_left,color-mix(in_oklch,var(--chart-1)_22%,transparent),transparent_34%),hsl(var(--surface))] p-8">
        <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          mxr wrapped
        </div>
        <div className="mt-6 text-4xl font-semibold tracking-tight">{totalMessages}</div>
        <div className="mt-2 text-sm text-muted-foreground">messages in this window</div>
      </section>
      <Panel title="Superlatives">
        <DataList rows={wrappedSuperlativeRows(summary)} />
      </Panel>
    </div>
  );
}

function Chart({
  rows,
  onClick,
}: {
  rows: Array<{ name: string; bytes: number; count: number }>;
  onClick: (row: { name: string; bytes: number; count: number }) => void;
}) {
  return (
    <div className="h-[360px]">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={rows}>
          <CartesianGrid stroke="var(--border)" vertical={false} />
          <XAxis dataKey="name" hide />
          <YAxis tickFormatter={formatBytes} width={72} />
          <Tooltip
            formatter={(value) => formatBytes(Number(value))}
            contentStyle={{
              background: "var(--popover)",
              border: "1px solid var(--border)",
              color: "var(--foreground)",
            }}
          />
          <Bar
            dataKey="bytes"
            fill="var(--chart-1)"
            radius={[6, 6, 0, 0]}
            onClick={(data) =>
              onClick(data.payload as { name: string; bytes: number; count: number })
            }
          />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

function Panel({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle>{title}</CardTitle>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

function Explainer({ children }: { children: React.ReactNode }) {
  return (
    <Alert role="note" variant="muted" className="mb-3 px-3 py-2">
      <AlertDescription>{children}</AlertDescription>
    </Alert>
  );
}

function DataList({ rows }: { rows: Array<{ id: string; title: string; meta: string }> }) {
  if (rows.length === 0)
    return (
      <div className="text-xs text-muted-foreground">
        No data yet. Run sync or rebuild analytics.
      </div>
    );
  return (
    <div className="divide-y divide-border">
      {rows.map((row) => (
        <div key={row.id} className="py-2">
          <div className="truncate text-xs font-medium">{row.title}</div>
          <div className="truncate text-2xs text-muted-foreground">{row.meta}</div>
        </div>
      ))}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <Card>
      <CardContent className="p-5">
        <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          {label}
        </div>
        <div className="mt-2 text-3xl font-semibold">{value}</div>
      </CardContent>
    </Card>
  );
}

function AnalyticsError({ error }: { error: Error | null }) {
  return (
    <EmptyState
      icon={RefreshCw}
      title="Analytics unavailable"
      description={error?.message ?? "Unknown error"}
    />
  );
}

function dashboardTitle(value: string) {
  const tab = analyticsTabs.find((item) => item.id === value);
  if (tab) return tab.label;
  return value
    .split("-")
    .map((part) => part[0]?.toUpperCase() + part.slice(1))
    .join(" ");
}

function rangeDays(range: AnalyticsRange) {
  if (range === "7d") return 7;
  if (range === "30d") return 30;
  if (range === "90d") return 90;
  return 365;
}

function groupByLabel(groupBy: StorageGroupBy) {
  if (groupBy === "mimetype") return "MIME type";
  return groupBy;
}

function subscriptionOpenRate(row: SubscriptionSummary): number {
  if (!row.message_count) return 0;
  return (row.opened_count ?? 0) / row.message_count;
}

function openRateLabel(row: SubscriptionSummary): string {
  return `${Math.round(subscriptionOpenRate(row) * 100)}%`;
}

function sortSubscriptions(rows: SubscriptionSummary[], sort: "low-open" | "volume" | "recent") {
  return rows.toSorted((a, b) => {
    if (sort === "volume") return b.message_count - a.message_count;
    if (sort === "recent")
      return String(b.latest_date ?? "").localeCompare(String(a.latest_date ?? ""));
    return subscriptionOpenRate(a) - subscriptionOpenRate(b) || b.message_count - a.message_count;
  });
}

function staleRow(row: {
  thread_id?: string;
  subject?: string;
  counterparty?: string;
  age_days?: number;
}) {
  return {
    id: row.thread_id ?? row.subject ?? "stale",
    title: row.subject ?? "(no subject)",
    meta: `${row.counterparty ?? "unknown"} · ${row.age_days ?? 0}d`,
  };
}

function contactRow(row: {
  email?: string;
  display_name?: string;
  inbound?: number;
  outbound?: number;
  total_inbound?: number;
  total_outbound?: number;
  days_since_last_seen?: number;
}) {
  return {
    id: row.email ?? row.display_name ?? "contact",
    title: row.display_name ?? row.email ?? "unknown",
    meta: `${row.inbound ?? row.total_inbound ?? 0} in · ${row.outbound ?? row.total_outbound ?? 0} out · ${row.days_since_last_seen ?? 0}d`,
  };
}

function wrappedSuperlativeRows(summary?: WrappedSummary) {
  const rows: Array<{ id: string; title: string; meta: string }> = [];
  const longest = summary?.superlatives?.longest_thread;
  if (longest) {
    rows.push({
      id: "longest-thread",
      title: "Longest thread",
      meta: `${longest.subject ?? "(no subject)"} · ${longest.message_count ?? 0} messages`,
    });
  }
  const ghosted = summary?.superlatives?.most_ghosted;
  if (ghosted) {
    rows.push({
      id: "most-ghosted",
      title: "Most ghosted",
      meta: `${ghosted.email ?? "unknown"} · ${ghosted.inbound_count ?? 0} in · ${ghosted.outbound_count ?? 0} out`,
    });
  }
  return rows;
}
