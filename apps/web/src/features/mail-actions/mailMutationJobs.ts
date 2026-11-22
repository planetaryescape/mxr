import { toast } from "sonner";

import { apiFetch } from "@/api/client";
import type { MutationResponse } from "@/features/mailbox/types";
import { plural } from "@/lib/format";
import { verb } from "./actionPastTense";
import type { MailAction, MailActionPayload } from "./pendingMailOps";

export const JOB_THRESHOLD = 200;

type JobMutationCommand = Record<string, unknown> & { mutation: string; message_ids: string[] };

export function jobCommand(
  action: MailAction,
  ids: string[],
  payload?: MailActionPayload,
): JobMutationCommand | null {
  const base = { message_ids: ids };
  switch (action) {
    case "archive":
      return { mutation: "Archive", ...base };
    case "read-and-archive":
      return { mutation: "ReadAndArchive", ...base };
    case "trash":
      return { mutation: "Trash", ...base };
    case "spam":
      return { mutation: "Spam", ...base };
    case "star":
    case "unstar":
      return { mutation: "Star", ...base, starred: action === "star" };
    case "read":
    case "unread":
      return { mutation: "SetRead", ...base, read: action === "read" };
    case "labels":
      return {
        mutation: "ModifyLabels",
        ...base,
        add: payload?.add ?? [],
        remove: payload?.remove ?? [],
      };
    case "label-add":
      return payload?.label
        ? { mutation: "ModifyLabels", ...base, add: [payload.label], remove: [] }
        : null;
    case "label-remove":
      return payload?.label
        ? { mutation: "ModifyLabels", ...base, add: [], remove: [payload.label] }
        : null;
    case "move":
      return payload?.label ? { mutation: "Move", ...base, target_label: payload.label } : null;
    default:
      return null;
  }
}

interface JobSnapshot {
  job_id: string;
  status: "queued" | "running" | "succeeded" | "failed";
  progress: {
    total: number;
    completed: number;
    succeeded: number;
    skipped: number;
    failed: number;
  };
  undo_ids?: string[];
  error?: string | null;
  result?: MutationResponse["result"];
}

export async function runAsJob(
  action: MailAction,
  command: JobMutationCommand,
  count: number,
  payload?: MailActionPayload,
): Promise<MutationResponse> {
  const started = await apiFetch<{ job_id: string }>("/api/v1/mail/mutation-jobs", {
    method: "POST",
    body: command,
  });
  const doing = verb(action, payload);
  return awaitJob(started.job_id, {
    start: `${doing} ${plural(count, "message")}…`,
    progress: (completed, total) =>
      `${doing} ${completed.toLocaleString()} of ${plural(total, "message")}`,
  });
}

export async function awaitJob(
  jobId: string,
  copy: { start: string; progress: (completed: number, total: number) => string },
): Promise<MutationResponse> {
  const toastId = `job-${jobId}`;
  toast.loading(copy.start, { id: toastId, description: "Running in the background" });
  for (;;) {
    // oxlint-disable-next-line no-await-in-loop
    await new Promise((resolve) => setTimeout(resolve, 600));
    // oxlint-disable-next-line no-await-in-loop
    const { job } = await apiFetch<{ job: JobSnapshot }>(
      `/api/v1/mail/jobs/${encodeURIComponent(jobId)}`,
    );
    const { completed, total } = job.progress;
    if (job.status === "queued" || job.status === "running") {
      toast.loading(copy.progress(completed, total), {
        id: toastId,
        description: "Running in the background",
      });
      continue;
    }
    toast.dismiss(toastId);
    const result = {
      ...(job.result ?? {
        requested: total,
        succeeded: job.progress.succeeded,
        skipped: job.progress.skipped,
        failed: job.progress.failed,
      }),
      undo_ids: job.undo_ids ?? [],
    };
    if (job.status === "failed" && job.error) {
      return {
        ok: false,
        result: {
          ...result,
          accounts: [
            {
              account_id: "",
              account_name: "job",
              succeeded: result.succeeded,
              skipped: result.skipped,
              failed: result.failed,
              error: job.error,
            },
          ],
        },
      };
    }
    return { ok: job.status === "succeeded", result };
  }
}
