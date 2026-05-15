import { useMutation, useQuery } from "@tanstack/react-query";
import { Activity, Bug, Clipboard, RefreshCw } from "lucide-react";
import type { ReactNode } from "react";
import { toast } from "sonner";

import { EventsPanel } from "./EventsPanel";
import { LogsPanel } from "./LogsPanel";
import {
  backfillSemantic,
  fetchAdminStatus,
  fetchBugReport,
  fetchDiagnostics,
  fetchSemanticStatus,
  fetchSyncStatus,
  installSemanticProfile,
  reindexSemantic,
  semanticProfiles,
  semanticSnapshot,
  setSemanticEnabled,
  useSemanticProfile,
  type SemanticProfile,
  type SemanticStatusSnapshot,
} from "./api";
import { ActivityBrowser } from "@/features/activity/ActivityRoute";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { fetchAccounts } from "@/features/accounts/api";

export function DiagnosticsRoute() {
  const status = useQuery({
    queryKey: ["diagnostics", "status"],
    queryFn: fetchAdminStatus,
    refetchInterval: 10_000,
  });
  const doctor = useQuery({ queryKey: ["diagnostics", "doctor"], queryFn: fetchDiagnostics });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const accountId = accounts.data?.accounts[0]?.account_id;
  const sync = useQuery({
    queryKey: ["diagnostics", "sync", accountId],
    queryFn: () => fetchSyncStatus(accountId ?? ""),
    enabled: Boolean(accountId),
    refetchInterval: 10_000,
  });
  const semantic = useQuery({
    queryKey: ["diagnostics", "semantic"],
    queryFn: fetchSemanticStatus,
    refetchInterval: 15_000,
  });
  const semanticStatus = semanticSnapshot(semantic.data);
  const bug = useMutation({
    mutationFn: fetchBugReport,
    onSuccess: async (result) => {
      await navigator.clipboard.writeText(result.content);
      toast.success("Bug report copied");
    },
    onError: (error) => toast.error("Bug report failed", { description: error.message }),
  });
  const semanticBackfill = useMutation({
    mutationFn: backfillSemantic,
    onSuccess: () => {
      toast.success("Semantic backfill queued");
      refreshSemanticHealth();
    },
    onError: (error) => toast.error("Semantic backfill failed", { description: error.message }),
  });
  const semanticEnable = useMutation({
    mutationFn: setSemanticEnabled,
    onSuccess: (_, enabled) => {
      toast.success(enabled ? "Semantic search enabled" : "Semantic search disabled");
      refreshSemanticHealth();
    },
    onError: (error) => toast.error("Semantic update failed", { description: error.message }),
  });
  const semanticReindex = useMutation({
    mutationFn: reindexSemantic,
    onSuccess: () => {
      toast.success("Semantic reindex queued");
      refreshSemanticHealth();
    },
    onError: (error) => toast.error("Semantic reindex failed", { description: error.message }),
  });
  const semanticInstall = useMutation({
    mutationFn: installSemanticProfile,
    onSuccess: (_, profile) => {
      toast.success(`${profile} install queued`);
      refreshSemanticHealth();
    },
    onError: (error) =>
      toast.error("Semantic profile install failed", { description: error.message }),
  });
  const semanticUse = useMutation({
    mutationFn: useSemanticProfile,
    onSuccess: (_, profile) => {
      toast.success(`${profile} selected`);
      refreshSemanticHealth();
    },
    onError: (error) =>
      toast.error("Semantic profile switch failed", { description: error.message }),
  });

  function refreshSemanticHealth() {
    void semantic.refetch();
    void doctor.refetch();
    void status.refetch();
  }

  if (status.isError && doctor.isError)
    return (
      <EmptyState
        icon={RefreshCw}
        title="Diagnostics unavailable"
        description={status.error.message}
      />
    );
  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="flex items-center gap-3 border-b border-border px-6 py-4">
        <div className="flex-1">
          <h1 className="text-xl font-semibold tracking-tight">Diagnostics</h1>
          <p className="text-2xs text-muted-foreground">
            Daemon health, logs, sync, semantic readiness.
          </p>
        </div>
        <Button variant="outline" onClick={() => bug.mutate()} disabled={bug.isPending}>
          <Bug className="size-3" />
          Copy bug report
        </Button>
      </header>
      <main className="min-h-0 flex-1 overflow-auto p-6">
        <Tabs defaultValue="overview" className="flex flex-col gap-4">
          <TabsList className="h-9 self-start">
            <TabsTrigger value="overview" className="text-2xs">
              Overview
            </TabsTrigger>
            <TabsTrigger value="logs" className="text-2xs">
              Logs
            </TabsTrigger>
            <TabsTrigger value="events" className="text-2xs">
              Events
            </TabsTrigger>
            <TabsTrigger value="activity" className="text-2xs">
              Activity
            </TabsTrigger>
          </TabsList>

          <TabsContent value="overview">
            <div className="grid gap-4 xl:grid-cols-2">
              <Panel title="Daemon status" icon={Activity} value={status.data} />
              <Panel
                title="Feature health"
                icon={Activity}
                value={status.data?.feature_health ?? doctor.data?.report?.feature_health}
              />
              <Panel title="Doctor report" icon={Clipboard} value={doctor.data?.report} />
              <Panel title="Sync status" icon={RefreshCw} value={sync.data} />
              <SemanticPanel
                status={semanticStatus}
                loading={semantic.isLoading}
                enablePending={semanticEnable.isPending}
                backfillPending={semanticBackfill.isPending}
                reindexPending={semanticReindex.isPending}
                installPending={semanticInstall.isPending}
                usePending={semanticUse.isPending}
                onSetEnabled={(enabled) => semanticEnable.mutate(enabled)}
                onBackfill={() => semanticBackfill.mutate()}
                onReindex={() => semanticReindex.mutate()}
                onInstall={(profile) => semanticInstall.mutate(profile)}
                onUse={(profile) => semanticUse.mutate(profile)}
              />
            </div>
          </TabsContent>

          <TabsContent value="logs">
            <LogsPanel />
          </TabsContent>

          <TabsContent value="events">
            <EventsPanel />
          </TabsContent>

          <TabsContent value="activity">
            <div className="rounded-xl border border-border bg-surface p-4">
              <div className="mb-2">
                <h2 className="text-sm font-semibold">Activity log</h2>
                <p className="text-2xs text-muted-foreground">
                  Local-only record of every user-initiated action across TUI, CLI, and web.
                  Strictly never transmitted off-device.
                </p>
              </div>
              <ActivityBrowser embedded />
            </div>
          </TabsContent>
        </Tabs>
      </main>
    </div>
  );
}

function SemanticPanel({
  status,
  loading,
  enablePending,
  backfillPending,
  reindexPending,
  installPending,
  usePending,
  onSetEnabled,
  onBackfill,
  onReindex,
  onInstall,
  onUse,
}: {
  status: SemanticStatusSnapshot | null;
  loading: boolean;
  enablePending: boolean;
  backfillPending: boolean;
  reindexPending: boolean;
  installPending: boolean;
  usePending: boolean;
  onSetEnabled: (enabled: boolean) => void;
  onBackfill: () => void;
  onReindex: () => void;
  onInstall: (profile: SemanticProfile) => void;
  onUse: (profile: SemanticProfile) => void;
}) {
  const profiles = new Map((status?.profiles ?? []).map((record) => [record.profile, record]));
  const busy = enablePending || backfillPending || reindexPending || installPending || usePending;
  return (
    <section className="rounded-xl border border-border bg-surface p-4">
      <div className="mb-3 flex items-center justify-between gap-3">
        <h2 className="flex items-center gap-2 text-sm font-semibold">
          <Activity className="size-3.5 text-primary" />
          Semantic controls
        </h2>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => onSetEnabled(!(status?.enabled ?? false))}
            disabled={!status || enablePending}
          >
            {status?.enabled ? "Disable semantic" : "Enable semantic"}
          </Button>
          <Button variant="outline" size="sm" onClick={onBackfill} disabled={backfillPending}>
            Backfill semantic
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={onReindex}
            disabled={!status || reindexPending}
          >
            Reindex active profile
          </Button>
        </div>
      </div>
      {!status ? (
        <div className="rounded-lg bg-muted p-3 text-2xs text-muted-foreground">
          {loading ? "Loading semantic status..." : "No semantic status."}
        </div>
      ) : (
        <div className="space-y-3">
          <div className="grid gap-2 rounded-lg bg-muted p-3 text-2xs md:grid-cols-4">
            <Metric label="Enabled" value={status.enabled ? "yes" : "no"} />
            <Metric label="Active profile" value={status.active_profile} />
            <Metric label="Queue" value={String(status.runtime?.queue_depth ?? 0)} />
            <Metric label="In flight" value={String(status.runtime?.in_flight ?? 0)} />
          </div>
          <div className="space-y-2">
            {semanticProfiles.map((profile) => {
              const record = profiles.get(profile);
              const active = status.active_profile === profile;
              return (
                <div
                  key={profile}
                  className="grid gap-2 rounded-lg border border-border bg-background p-3 md:grid-cols-[1fr_auto] md:items-center"
                >
                  <div>
                    <div className="font-mono text-xs text-foreground">{profile}</div>
                    <div className="mt-1 text-2xs text-muted-foreground">
                      {record
                        ? `${record.status} · ${record.backend} · ${record.dimensions} dims · ${record.progress_completed}/${record.progress_total}`
                        : "Not installed"}
                    </div>
                    {record?.last_error ? (
                      <div className="mt-1 text-2xs text-destructive">{record.last_error}</div>
                    ) : null}
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => onInstall(profile)}
                      disabled={busy}
                    >
                      Install
                    </Button>
                    <Button
                      variant={active ? "secondary" : "outline"}
                      size="sm"
                      onClick={() => onUse(profile)}
                      disabled={busy || active || !record}
                    >
                      {active ? "Active" : "Use"}
                    </Button>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </section>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="font-mono uppercase tracking-wide text-muted-foreground">{label}</div>
      <div className="mt-1 font-mono text-foreground">{value}</div>
    </div>
  );
}

function Panel({
  title,
  icon: Icon,
  value,
  action,
  wide,
}: {
  title: string;
  icon: typeof Activity;
  value: unknown;
  action?: ReactNode;
  wide?: boolean;
}) {
  return (
    <section
      className={
        wide
          ? "rounded-xl border border-border bg-surface p-4 xl:col-span-2"
          : "rounded-xl border border-border bg-surface p-4"
      }
    >
      <div className="mb-3 flex items-center justify-between gap-3">
        <h2 className="flex items-center gap-2 text-sm font-semibold">
          <Icon className="size-3.5 text-primary" />
          {title}
        </h2>
        {action}
      </div>
      <DiagnosticValue value={value} />
    </section>
  );
}

export function DiagnosticValue({ value }: { value: unknown }) {
  if (value === null || value === undefined) {
    return <div className="rounded-lg bg-muted p-3 text-2xs text-muted-foreground">No data.</div>;
  }

  if (Array.isArray(value)) {
    if (value.length === 0) {
      return (
        <div className="rounded-lg bg-muted p-3 text-2xs text-muted-foreground">No entries.</div>
      );
    }
    return (
      <div className="max-h-[360px] overflow-auto rounded-lg bg-muted p-3">
        <div className="divide-y divide-border/70">
          {value.map((item, index) => (
            <div key={diagnosticKey(item, index)} className="py-1.5 text-2xs">
              {typeof item === "object" && item !== null ? (
                <DiagnosticObject value={item as Record<string, unknown>} compact />
              ) : (
                <span className="font-mono text-muted-foreground">{diagnosticScalar(item)}</span>
              )}
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (typeof value === "object") {
    return (
      <div className="max-h-[360px] overflow-auto rounded-lg bg-muted p-3">
        <DiagnosticObject value={value as Record<string, unknown>} />
      </div>
    );
  }

  return (
    <div className="rounded-lg bg-muted p-3 font-mono text-2xs text-muted-foreground">
      {diagnosticScalar(value)}
    </div>
  );
}

function DiagnosticObject({
  value,
  compact,
}: {
  value: Record<string, unknown>;
  compact?: boolean;
}) {
  const entries = Object.entries(value);
  if (entries.length === 0) return <div className="text-2xs text-muted-foreground">Empty.</div>;
  return (
    <dl className={compact ? "grid gap-1" : "grid gap-2"}>
      {entries.map(([key, item]) => (
        <div
          key={key}
          className={
            compact
              ? "grid grid-cols-[120px_1fr] gap-2"
              : "grid grid-cols-[minmax(120px,180px)_1fr] gap-3"
          }
        >
          <dt className="min-w-0 truncate font-mono text-2xs text-muted-foreground">{key}</dt>
          <dd className="min-w-0 break-words font-mono text-2xs text-foreground">
            {diagnosticScalar(item)}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function diagnosticScalar(value: unknown): string {
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint") {
    return String(value);
  }
  if (Array.isArray(value)) return `${value.length} entries`;
  return JSON.stringify(value);
}

function diagnosticKey(value: unknown, index: number): string {
  if (typeof value === "object" && value !== null) {
    const record = value as Record<string, unknown>;
    const id = record.id ?? record.event_id ?? record.timestamp ?? record.time;
    if (typeof id === "string" || typeof id === "number") return String(id);
  }
  return String(index);
}
