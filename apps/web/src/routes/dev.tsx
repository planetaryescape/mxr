import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Activity, AlertTriangle, CheckCircle2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { apiFetch } from "@/api/client";
import { useBridgeToken } from "@/hooks/useBridgeToken";
import { useConnectionStore } from "@/state/connectionStore";

export const Route = createFileRoute("/dev")({
  component: DevPage,
});

function DevPage() {
  const { token, hasToken } = useBridgeToken();
  const connState = useConnectionStore((s) => s.state);

  const health = useQuery({
    queryKey: ["dev", "health"],
    queryFn: () =>
      apiFetch<{ status: string; service: string; protocol_version: number }>(
        "/api/v1/admin/health",
      ),
    refetchInterval: 5_000,
  });

  const status = useQuery({
    queryKey: ["dev", "status"],
    queryFn: () => apiFetch<unknown>("/api/v1/admin/status"),
    enabled: hasToken,
    retry: false,
  });

  return (
    <div className="flex h-full w-full flex-col">
      <div className="border-b border-border px-6 py-4">
        <h1 className="text-md font-semibold">/dev — bridge smoke check</h1>
        <p className="mt-1 text-2xs text-muted-foreground">
          Verifies the SPA can reach the daemon bridge end-to-end. Lives in dev builds only.
        </p>
      </div>
      <ScrollArea className="flex-1">
        <div className="grid gap-4 p-6">
          <Card title="Bridge token">
            <KeyValue k="present" v={hasToken ? "yes" : "no"} status={hasToken ? "ok" : "warn"} />
            {hasToken ? (
              <KeyValue k="token" v={`${token.slice(0, 8)}…${token.slice(-4)}`} tone="muted" />
            ) : (
              <p className="text-2xs text-warning">
                No token in localStorage. Re-run <code>mxr web</code> or paste a token in
                <code> /settings/token</code>.
              </p>
            )}
          </Card>

          <Card title="GET /api/v1/admin/health (unauthenticated)">
            {health.isLoading ? <Spinner /> : null}
            {health.error ? <ErrorRow error={health.error} /> : null}
            {health.data ? (
              <>
                <KeyValue k="status" v={health.data.status} status="ok" />
                <KeyValue k="service" v={health.data.service} />
                <KeyValue k="protocol_version" v={String(health.data.protocol_version)} />
              </>
            ) : null}
          </Card>

          <Card title="GET /api/v1/admin/status (authenticated)">
            {!hasToken ? (
              <p className="text-2xs text-muted-foreground">Skipped — no token.</p>
            ) : status.isLoading ? (
              <Spinner />
            ) : status.error ? (
              <ErrorRow error={status.error} />
            ) : (
              <pre className="max-h-[40vh] overflow-auto rounded-md border border-border bg-surface p-3 font-mono text-2xs">
                {JSON.stringify(status.data, null, 2)}
              </pre>
            )}
            <Button size="sm" variant="outline" onClick={() => status.refetch()} className="mt-2">
              Refetch
            </Button>
          </Card>

          <Card title="WebSocket /api/v1/events">
            <KeyValue
              k="state"
              v={connState}
              status={connState === "connected" ? "ok" : connState === "offline" ? "err" : "warn"}
            />
          </Card>
        </div>
      </ScrollArea>
    </div>
  );
}

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-md border border-border bg-surface p-4">
      <h2 className="mb-3 flex items-center gap-2 text-xs font-semibold">
        <Activity className="size-3 text-muted-foreground" /> {title}
      </h2>
      <div className="space-y-1">{children}</div>
    </section>
  );
}

function KeyValue({
  k,
  v,
  status,
  tone,
}: {
  k: string;
  v: string;
  status?: "ok" | "warn" | "err";
  tone?: "muted";
}) {
  const Icon =
    status === "ok"
      ? CheckCircle2
      : status === "warn"
        ? AlertTriangle
        : status === "err"
          ? AlertTriangle
          : null;
  const color =
    status === "ok"
      ? "text-success"
      : status === "warn"
        ? "text-warning"
        : status === "err"
          ? "text-destructive"
          : "text-foreground";
  return (
    <div className="flex items-center gap-2 font-mono text-2xs">
      <span className="w-32 shrink-0 text-muted-foreground">{k}</span>
      <span className={tone === "muted" ? "text-muted-foreground" : color}>{v}</span>
      {Icon ? <Icon className={`size-3 ${color}`} /> : null}
    </div>
  );
}

function ErrorRow({ error }: { error: unknown }) {
  return (
    <div className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/10 p-2 text-2xs text-destructive">
      <AlertTriangle className="size-3 shrink-0" />
      <span className="font-mono">{error instanceof Error ? error.message : String(error)}</span>
    </div>
  );
}

function Spinner() {
  return <span className="text-2xs text-muted-foreground">loading…</span>;
}
