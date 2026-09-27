/*
 * Diagnostics overview: daemon status, per-account sync health, feature
 * health and the doctor report, read from the admin endpoints.
 */

import type { UseQueryResult } from "@tanstack/react-query";

import { DiagnosticValue } from "./DiagnosticValue";
import { PageSection } from "@/components/Page";
import { FactList, PageError, PageSkeleton, RuledList, RuledRow } from "@/components/PageParts";
import { formatRelative, plural } from "@/lib/format";
import { formatBytes } from "@/lib/utils";

interface SyncStatusRow {
  account_id: string;
  account_name?: string;
  healthy?: boolean;
  sync_in_progress?: boolean;
  last_success_at?: string | null;
  last_error?: string | null;
  consecutive_failures?: number;
  backoff_until?: string | null;
}

interface FeatureHealth {
  status?: string;
  reason?: string;
}

type Record_ = Record<string, unknown>;

function str(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function num(value: unknown): number | undefined {
  return typeof value === "number" ? value : undefined;
}

function formatUptime(seconds: number | undefined): string {
  if (seconds === undefined) return "n/a";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}

function healthTone(status: string | undefined): string {
  if (status === "healthy") return "text-success";
  if (status === "degraded") return "text-warning";
  if (status === "failed" || status === "unhealthy") return "text-destructive";
  return "text-muted-foreground";
}

export function OverviewPanel({
  status,
  doctor,
}: {
  status: UseQueryResult<Record_>;
  doctor: UseQueryResult<{ report: Record_ }>;
}) {
  const data = status.data;
  const syncRows = Array.isArray(data?.sync_statuses)
    ? (data.sync_statuses as SyncStatusRow[])
    : [];
  const features = (data?.feature_health ?? doctor.data?.report.feature_health) as
    | Record<string, FeatureHealth>
    | undefined;
  const report = doctor.data?.report;
  const findings = Array.isArray(report?.findings) ? (report.findings as unknown[]) : [];

  return (
    <div className="grid gap-x-10 xl:grid-cols-2">
      <PageSection title="Daemon">
        {status.isPending ? (
          <PageSkeleton rows={3} label="Loading daemon status" />
        ) : status.isError ? (
          <PageError
            title="Daemon status unavailable"
            error={status.error}
            onRetry={() => void status.refetch()}
          />
        ) : (
          <FactList
            facts={[
              ["Version", str(data?.daemon_version) ?? "n/a"],
              ["Uptime", formatUptime(num(data?.uptime_secs))],
              ["Messages", (num(data?.total_messages) ?? 0).toLocaleString()],
              ["Protocol", String(num(data?.protocol_version) ?? "n/a")],
              [
                "Health",
                <span key="health" className={data?.degraded ? "text-warning" : "text-success"}>
                  {data?.repair_required
                    ? "repair required"
                    : data?.degraded
                      ? "degraded"
                      : "healthy"}
                </span>,
              ],
              ["PID", String(num(data?.daemon_pid) ?? "n/a")],
            ]}
          />
        )}
      </PageSection>

      <PageSection title="Sync">
        {status.isPending ? (
          <PageSkeleton rows={2} label="Loading sync status" />
        ) : syncRows.length === 0 ? (
          <p className="text-[13px] text-muted-foreground">No accounts are syncing.</p>
        ) : (
          <RuledList label="Sync status by account">
            {syncRows.map((row) => (
              <RuledRow
                key={row.account_id}
                title={row.account_name ?? row.account_id}
                meta={
                  row.last_error
                    ? row.last_error
                    : `last synced ${row.last_success_at ? formatRelative(row.last_success_at) : "never"}${row.consecutive_failures ? ` · ${plural(row.consecutive_failures, "failure")} in a row` : ""}`
                }
                aside={
                  <span className={row.healthy ? "text-success" : "text-destructive"}>
                    {row.sync_in_progress ? "syncing" : row.healthy ? "healthy" : "failing"}
                  </span>
                }
              />
            ))}
          </RuledList>
        )}
      </PageSection>

      <PageSection title="Features">
        {features ? (
          <RuledList label="Feature health">
            {Object.entries(features).map(([name, health]) => (
              <RuledRow
                key={name}
                title={name.replace(/_/g, " ")}
                meta={health.reason}
                aside={
                  <span className={healthTone(health.status)}>{health.status ?? "unknown"}</span>
                }
              />
            ))}
          </RuledList>
        ) : status.isPending || doctor.isPending ? (
          <PageSkeleton rows={4} label="Loading feature health" />
        ) : (
          <p className="text-[13px] text-muted-foreground">No feature health reported.</p>
        )}
      </PageSection>

      <PageSection title="Doctor">
        {doctor.isPending ? (
          <PageSkeleton rows={4} label="Loading doctor report" />
        ) : doctor.isError ? (
          <PageError
            title="Doctor report unavailable"
            error={doctor.error}
            onRetry={() => void doctor.refetch()}
          />
        ) : report ? (
          <div className="space-y-4">
            <FactList
              facts={[
                [
                  "Result",
                  <span key="result" className={healthTone(str(report.health_class))}>
                    {str(report.health_class) ?? (report.healthy ? "healthy" : "unhealthy")}
                  </span>,
                ],
                ["Findings", findings.length === 0 ? "none" : plural(findings.length, "finding")],
                ["Database", formatBytes(num(report.database_size_bytes) ?? 0)],
                ["Index", report.index_exists ? "present" : "missing"],
              ]}
            />
            {findings.length > 0 ? <DiagnosticValue value={findings} /> : null}
            <details>
              <summary className="cursor-pointer font-mono text-2xs text-muted-foreground hover:text-foreground">
                Full report
              </summary>
              <div className="mt-2">
                <DiagnosticValue value={report} />
              </div>
            </details>
          </div>
        ) : null}
      </PageSection>
    </div>
  );
}
