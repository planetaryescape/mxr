import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { llmPolicyKey, useLlmStatus } from "@/features/llm/useLlmStatus";
import { useThreadModes } from "@/features/modes/membership";
import { fetchThread } from "@/features/mailbox/api";
import { PageError } from "@/components/PageParts";
import { useOpenThread } from "@/state/openThreadStore";

import { threadContextQuery, threadGistQuery } from "./context/api";
import { useDelayedPending } from "@/hooks/useDelayedPending";

import { ReaderSkeleton } from "./ReaderSkeleton";
import type { LinkHighlight } from "./linkHighlight";
import { ThreadReader } from "./ThreadReader";

export function ThreadPane({
  threadId,
  focusMessageId,
  linkHighlight,
}: {
  threadId: string;
  focusMessageId?: string;
  linkHighlight?: LinkHighlight;
}) {
  const query = useQuery({
    queryKey: ["thread", threadId, "page", focusMessageId ?? ""],
    queryFn: () =>
      fetchThread(threadId, focusMessageId ? { focus: focusMessageId } : {}),
  });
  const context = useQuery(threadContextQuery(threadId));
  const modes = useThreadModes(threadId);
  const llm = useLlmStatus();
  const policy = llmPolicyKey(llm.data?.status);
  const queryClient = useQueryClient();
  useEffect(() => {
    if (llm.enabled) void queryClient.prefetchQuery(threadGistQuery(threadId, policy));
  }, [llm.enabled, policy, queryClient, threadId]);
  const phase = useDelayedPending(
    !query.isError && (query.isLoading || context.isLoading || modes.isLoading),
  );
  const setOpenThread = useOpenThread((s) => s.setThreadId);
  useEffect(() => {
    setOpenThread(threadId);
    return () => setOpenThread(null);
  }, [setOpenThread, threadId]);

  if (query.isError) {
    return (
      <div className="flex min-w-0 flex-1 flex-col">
        <PageError
          framed
          title="Couldn't open this conversation"
          subject="this conversation"
          error={query.error}
          onRetry={() => void query.refetch()}
        />
      </div>
    );
  }
  if (phase !== "ready") return <ReaderSkeleton quiet={phase === "quiet"} />;
  if (!query.data) return null;
  return (
    <ThreadReader
      key={`${query.data.thread.id}:${focusMessageId ?? ""}`}
      data={query.data}
      focusMessageId={focusMessageId}
      linkHighlight={linkHighlight}
    />
  );
}
