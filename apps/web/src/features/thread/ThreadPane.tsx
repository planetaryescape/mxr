import { useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import { llmPolicyKey, useLlmStatus } from "@/features/llm/useLlmStatus";
import { fetchThread } from "@/features/mailbox/api";
import { Centered } from "@/features/mailbox/MailViewParts";
import { useOpenThread } from "@/state/openThreadStore";

import { threadContextQuery, threadGistQuery } from "./context/api";
import { ReaderSkeleton } from "./ReaderSkeleton";
import { ThreadReader } from "./ThreadReader";

/** The reader pane for one conversation, opened from a list (see MailView). */
export function ThreadPane({ threadId }: { threadId: string }) {
  const query = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
  });
  // Loaded alongside the thread, and waited for, so the context block is in
  // place before the first message paints. It's a local read like the
  // thread; a failed read just leaves the block out. The model status is
  // never waited for: reading must not depend on it (AppShell fetches it at
  // start, so the gist slot is usually decided by the time a thread opens).
  const context = useQuery(threadContextQuery(threadId));
  const llm = useLlmStatus();
  const policy = llmPolicyKey(llm.data?.status);
  // Ask for the gist as soon as the thread is known, not after the reader
  // mounts: the model is the slow part.
  const queryClient = useQueryClient();
  useEffect(() => {
    if (llm.enabled) void queryClient.prefetchQuery(threadGistQuery(threadId, policy));
  }, [llm.enabled, policy, queryClient, threadId]);
  const setOpenThread = useOpenThread((s) => s.setThreadId);
  useEffect(() => {
    setOpenThread(threadId);
    return () => setOpenThread(null);
  }, [setOpenThread, threadId]);

  // The thread's own failure shows at once, whatever the context request is
  // doing. On success the reader still waits for the context, so the block
  // above the messages doesn't shift them once it lands.
  if (query.isError) {
    return (
      <div className="flex min-w-0 flex-1 flex-col">
        <Centered
          icon={<RefreshCw className="size-6" />}
          title="Couldn't open this conversation"
          body={query.error.message}
          action={
            <Button size="sm" onClick={() => void query.refetch()}>
              Try again
            </Button>
          }
        />
      </div>
    );
  }
  if (query.isLoading || context.isLoading) return <ReaderSkeleton />;
  if (!query.data) return null;
  // No placeholder data: while the next thread loads, the previous thread's
  // controller must not stay mounted, or keys would act on the wrong thread.
  // Key by thread so per-thread state (expanded, remote images) resets.
  return <ThreadReader key={query.data.thread.id} data={query.data} />;
}
