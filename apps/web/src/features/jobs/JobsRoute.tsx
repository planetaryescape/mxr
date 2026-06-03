import { useQuery } from "@tanstack/react-query";
import { Loader2, RefreshCcw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { fetchJobs, type JobData } from "./api";

export function JobsRoute() {
  const query = useQuery({
    queryKey: ["jobs"],
    queryFn: fetchJobs,
    refetchInterval: 2_000,
  });

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-6">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">Jobs</h1>
          <p className="text-sm text-muted-foreground">
            Background mutation progress, failures, and undo ids.
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={() => void query.refetch()}>
          {query.isFetching ? <Loader2 className="mr-2 size-4 animate-spin" /> : <RefreshCcw className="mr-2 size-4" />}
          Refresh
        </Button>
      </div>

      {query.isError ? (
        <Card><CardContent className="p-4 text-sm text-destructive">{String(query.error)}</CardContent></Card>
      ) : null}

      <div className="grid gap-3">
        {(query.data ?? []).map((job) => <JobCard key={job.job_id} job={job} />)}
        {query.isLoading ? <Card><CardContent className="p-4 text-sm">Loading jobs…</CardContent></Card> : null}
        {!query.isLoading && (query.data ?? []).length === 0 ? (
          <Card><CardContent className="p-4 text-sm text-muted-foreground">No jobs yet.</CardContent></Card>
        ) : null}
      </div>
    </div>
  );
}

function JobCard({ job }: { job: JobData }) {
  const pct = job.progress.total > 0 ? Math.round((job.progress.completed / job.progress.total) * 100) : 0;
  return (
    <Card>
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between gap-2">
          <CardTitle className="text-base">{job.kind}</CardTitle>
          <Badge variant={job.status === "failed" ? "destructive" : "secondary"}>{job.status}</Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-3 text-sm">
        <div className="font-mono text-xs text-muted-foreground">{job.job_id}</div>
        <div>
          <div className="mb-1 flex justify-between text-xs text-muted-foreground">
            <span>{job.progress.completed}/{job.progress.total}</span>
            <span>{pct}%</span>
          </div>
          <div className="h-2 rounded bg-muted">
            <div className="h-2 rounded bg-primary" style={{ width: `${pct}%` }} />
          </div>
        </div>
        <div className="grid gap-1 sm:grid-cols-4">
          <span>succeeded: {job.progress.succeeded}</span>
          <span>skipped: {job.progress.skipped}</span>
          <span>failed: {job.progress.failed}</span>
          <span>undo ids: {job.undo_ids.length}</span>
        </div>
        {job.undo_ids.length > 0 ? <div className="font-mono text-xs">{job.undo_ids.join(", ")}</div> : null}
        {job.error ? <div className="text-destructive">{job.error}</div> : null}
      </CardContent>
    </Card>
  );
}
