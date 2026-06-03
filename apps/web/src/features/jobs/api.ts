import { apiFetch } from "@/api/client";

export type JobStatus = "queued" | "running" | "succeeded" | "failed";

export interface JobProgress {
  total: number;
  completed: number;
  succeeded: number;
  skipped: number;
  failed: number;
}

export interface MutationAccountResult {
  account_id: string;
  account_name: string;
  succeeded: number;
  skipped: number;
  failed: number;
  error: string | null;
}

export interface MutationResult {
  requested: number;
  succeeded: number;
  skipped: number;
  failed: number;
  accounts: MutationAccountResult[];
  mutation_id?: string | null;
}

export interface JobData {
  job_id: string;
  kind: string;
  status: JobStatus;
  progress: JobProgress;
  undo_ids: string[];
  error?: string | null;
  started_at: number;
  finished_at?: number | null;
  result?: MutationResult | null;
}

export interface JobsResponse {
  Jobs?: { jobs: JobData[] };
  jobs?: JobData[];
}

function unwrapJobs(response: JobsResponse): JobData[] {
  return response.jobs ?? response.Jobs?.jobs ?? [];
}

export async function fetchJobs(): Promise<JobData[]> {
  return unwrapJobs(await apiFetch<JobsResponse>("/api/v1/mail/jobs"));
}
