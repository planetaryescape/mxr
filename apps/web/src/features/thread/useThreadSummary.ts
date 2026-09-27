/*
 * The conversation overview: the daemon's cached summary, on-demand
 * summarize, and the automatic one for long conversations.
 */

import { useMutation } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
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
  const summarizeRef = useRef(summarize.mutate);
  summarizeRef.current = summarize.mutate;
  // Long conversations get an overview automatically (TUI: 200+ words),
  // only when the daemon has a model to ask.
  const llm = useLlmStatus();
  useEffect(() => {
    if (summary || !llm.enabled) return;
    const words = data.bodies.reduce(
      (total, body) => total + (body.reader_text ?? body.text_plain ?? "").split(/\s+/).length,
      0,
    );
    if (words < 200 || data.messages.length < 2) return;
    const handle = window.setTimeout(() => summarizeRef.current({ silent: true }), 400);
    return () => window.clearTimeout(handle);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data.thread.id, llm.enabled]);

  return { summary, setSummary, summarize, llm };
}
