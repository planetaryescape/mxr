/*
 * The conversation overview: the daemon's cached summary and on-demand
 * summarize (`y`).
 */

import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { summarizeThread } from "@/features/mailbox/api";
import type { ThreadResponse } from "@/features/mailbox/types";
import { normalizeThreadSummary, type ThreadSummaryView } from "./ThreadInsights";

export function useThreadSummary(data: ThreadResponse) {
  const [summary, setSummary] = useState<ThreadSummaryView | null>(() =>
    normalizeThreadSummary(data.summary ? { summary: data.summary } : null),
  );

  const summarize = useMutation({
    mutationFn: () => summarizeThread(data.thread.id),
    onSuccess: (result) => {
      const next = normalizeThreadSummary(result);
      if (next) setSummary(next);
      else toast.error("Summary failed", { description: "The daemon returned no summary." });
    },
    onError: (error) => toast.error("Summary failed", { description: error.message }),
  });
  return { summary, setSummary, summarize };
}
