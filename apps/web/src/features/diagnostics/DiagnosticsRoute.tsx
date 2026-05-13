import { useMutation, useQuery } from "@tanstack/react-query";
import { Activity, Bug, Clipboard, RefreshCw } from "lucide-react";
import { toast } from "sonner";

import {
  fetchAdminStatus,
  fetchBugReport,
  fetchDiagnostics,
  fetchEvents,
  fetchLogs,
  fetchSemanticStatus,
  fetchSyncStatus,
} from "./api";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";
import { fetchAccounts } from "@/features/accounts/api";

export function DiagnosticsRoute() {
  const status = useQuery({
    queryKey: ["diagnostics", "status"],
    queryFn: fetchAdminStatus,
    refetchInterval: 10_000,
  });
  const doctor = useQuery({ queryKey: ["diagnostics", "doctor"], queryFn: fetchDiagnostics });
  const logs = useQuery({
    queryKey: ["diagnostics", "logs"],
    queryFn: () => fetchLogs(120),
    refetchInterval: 5_000,
  });
  const events = useQuery({
    queryKey: ["diagnostics", "events"],
    queryFn: () => fetchEvents(50),
    refetchInterval: 5_000,
  });
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
  const bug = useMutation({
    mutationFn: fetchBugReport,
    onSuccess: async (result) => {
      await navigator.clipboard.writeText(result.content);
      toast.success("Bug report copied");
    },
    onError: (error) => toast.error("Bug report failed", { description: error.message }),
  });

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
      <main className="grid min-h-0 gap-4 overflow-auto p-6 xl:grid-cols-2">
        <Panel title="Daemon status" icon={Activity} value={status.data} />
        <Panel title="Doctor report" icon={Clipboard} value={doctor.data?.report} />
        <Panel title="Sync status" icon={RefreshCw} value={sync.data} />
        <Panel title="Semantic status" icon={Activity} value={semantic.data} />
        <Panel title="Recent logs" icon={Clipboard} value={logs.data?.lines ?? logs.data} wide />
        <Panel
          title="Recent events"
          icon={Activity}
          value={events.data?.entries ?? events.data}
          wide
        />
      </main>
    </div>
  );
}

function Panel({
  title,
  icon: Icon,
  value,
  wide,
}: {
  title: string;
  icon: typeof Activity;
  value: unknown;
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
      <h2 className="mb-3 flex items-center gap-2 text-sm font-semibold">
        <Icon className="size-3.5 text-primary" />
        {title}
      </h2>
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
