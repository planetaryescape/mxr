import { useQuery } from "@tanstack/react-query";
import { ListChecks } from "lucide-react";

import { fetchJobs, type JobData } from "./api";
import { Page } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton, RuledList } from "@/components/PageParts";
import { formatRelative, plural } from "@/lib/format";
import { cn } from "@/lib/utils";

const STATUS_TONE: Record<string, string> = {
  queued: "text-muted-foreground",
  running: "text-primary",
  succeeded: "text-success",
  failed: "text-destructive",
};

export function JobsRoute() {
  const jobs = useQuery({
    queryKey: ["jobs"],
    queryFn: fetchJobs,
    // Poll only while something is in flight.
    refetchInterval: (query) =>
      (query.state.data ?? []).some((job) => job.status === "queued" || job.status === "running")
        ? 2_000
        : 15_000,
  });

  return (
    <Page title="Jobs" description="Long-running mail operations: progress, failures and undo ids.">
      {jobs.isPending ? (
        <PageSkeleton rows={4} label="Loading jobs" />
      ) : jobs.isError ? (
        <PageError
          title="Jobs unavailable"
          error={jobs.error}
          onRetry={() => void jobs.refetch()}
        />
      ) : jobs.data.length === 0 ? (
        <PageEmpty
          icon={<ListChecks className="size-5" />}
          title="No background jobs"
          body="Bulk actions over many messages run here so you can watch their progress."
        />
      ) : (
        <RuledList label="Jobs">
          {jobs.data.map((job) => (
            <JobRow key={job.job_id} job={job} />
          ))}
        </RuledList>
      )}
    </Page>
  );
}

function JobRow({ job }: { job: JobData }) {
  const { progress } = job;
  const pct = progress.total > 0 ? Math.round((progress.completed / progress.total) * 100) : 0;
  return (
    <li className="border-b border-border/60 px-2 py-3">
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-[13px] font-medium">{job.kind.replace(/_/g, " ")}</span>
        <span className={cn("font-mono text-2xs", STATUS_TONE[job.status])}>{job.status}</span>
      </div>
      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-muted" aria-hidden="true">
        <div
          className={cn("h-full", job.status === "failed" ? "bg-destructive" : "bg-primary")}
          style={{ width: `${pct}%` }}
        />
      </div>
      <div className="mt-1.5 flex flex-wrap gap-x-3 font-mono text-2xs text-muted-foreground">
        <span>
          {progress.completed.toLocaleString()} of {plural(progress.total, "message")} ({pct}%)
        </span>
        <span>{progress.succeeded.toLocaleString()} done</span>
        {progress.skipped > 0 ? <span>{progress.skipped.toLocaleString()} skipped</span> : null}
        {progress.failed > 0 ? (
          <span className="text-destructive">{progress.failed.toLocaleString()} failed</span>
        ) : null}
        <span>started {formatRelative(new Date(job.started_at))}</span>
        {job.undo_ids.length > 0 ? <span>{plural(job.undo_ids.length, "undo id")}</span> : null}
      </div>
      {job.error ? <p className="mt-1.5 text-[12.5px] text-destructive">{job.error}</p> : null}
    </li>
  );
}
