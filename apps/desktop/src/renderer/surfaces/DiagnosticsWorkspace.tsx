import { MailWarning } from "lucide-react";
import type { BridgeState, DiagnosticsResponse } from "../../shared/types";
import { HeaderActionButton, StatCard } from "./shared";

export function DiagnosticsWorkspace(props: {
  bridge: Extract<BridgeState, { kind: "ready" }>;
  diagnostics: DiagnosticsResponse | null;
  onGenerateBugReport: () => void;
}) {
  return (
    <div className="grid h-full place-items-center bg-panel-muted px-4 py-4">
      <section className="surface flex w-full max-w-4xl flex-col gap-4 px-4 py-4">
        <div className="flex items-center gap-3">
          <div className="border border-outline bg-panel-elevated p-2">
            <MailWarning className="size-5 text-warning" />
          </div>
          <div>
            <p className="mono-meta">Diagnostics</p>
            <h1 className="mt-1 text-2xl font-semibold tracking-tight text-foreground">
              Diagnostics
            </h1>
          </div>
        </div>
        <div className="flex justify-end">
          <HeaderActionButton label="Generate bug report" onClick={props.onGenerateBugReport} />
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          <StatCard label="Daemon version" value={props.bridge.daemonVersion ?? "unknown"} />
          <StatCard label="Protocol" value={String(props.bridge.protocolVersion)} />
          <StatCard label="Health" value={props.diagnostics?.report.health_class ?? "loading"} />
        </div>
        <div className="grid gap-4 md:grid-cols-2">
          <div className="border border-outline bg-panel-elevated px-3 py-3">
            <p className="mono-meta">Recommended next steps</p>
            <div className="mt-3 space-y-2">
              {(props.diagnostics?.report.recommended_next_steps ?? []).length === 0 ? (
                <p className="text-sm text-foreground-muted">No follow-up actions reported.</p>
              ) : (
                props.diagnostics?.report.recommended_next_steps.map((item) => (
                  <p key={item} className="text-sm leading-6 text-foreground-muted">
                    {item}
                  </p>
                ))
              )}
            </div>
          </div>
          <div className="border border-outline bg-panel-elevated px-3 py-3">
            <p className="mono-meta">Recent errors</p>
            <div className="mt-3 space-y-2">
              {(props.diagnostics?.report.recent_error_logs ?? []).length === 0 ? (
                <p className="text-sm text-foreground-muted">No recent error logs.</p>
              ) : (
                props.diagnostics?.report.recent_error_logs.map((item) => (
                  <p key={item} className="font-mono text-xs leading-6 text-foreground-muted">
                    {item}
                  </p>
                ))
              )}
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}
