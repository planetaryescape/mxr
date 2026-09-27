/*
 * The conversation overview: the daemon's cached summary and on-demand
 * summarize (`y`).
 */

import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { summarizeThread } from "@/features/mailbox/api";
import type { ThreadResponse } from "@/features/mailbox/types";
import { useLlmStatus } from "@/features/llm/useLlmStatus";
import { normalizeThreadSummary, type ThreadSummaryView } from "./ThreadInsights";

export function useThreadSummary(data: ThreadResponse) {
  const [summary, setSummary] = useState<ThreadSummaryView | null>(() =>
    normalizeThreadSummary(data.summary ? { summary: data.summary } : null),
  );

  const summarize = useMutation({
    mutationFn: (_input: { silent: boolean }) => summarizeThread(data.thread.id),
    onSuccess: (result, input) => {
      const next = normalizeThreadSummary(result);
      if (next) setSummary(next);
      else if (!input.silent)
        toast.error("Summary failed", { description: "The daemon returned no summary." });
    },
    onError: (error, input) => {
      if (!input.silent) toast.error("Summary failed", { description: error.message });
    },
  });
  // Long threads get no automatic overview: the context block's gist covers
  // what they are about, and `y` asks for the full summary.
  const llm = useLlmStatus();

  return { summary, setSummary, summarize, llm };
}
