import { useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import { fetchThread } from "@/features/mailbox/api";
import { Centered } from "@/features/mailbox/MailViewParts";
import { useOpenThread } from "@/state/openThreadStore";

import { ReaderSkeleton } from "./ReaderSkeleton";
import { ThreadReader } from "./ThreadReader";

/** The reader pane for one conversation, opened from a list (see MailView). */
export function ThreadPane({ threadId }: { threadId: string }) {
  const query = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
  });
  const setOpenThread = useOpenThread((s) => s.setThreadId);
  useEffect(() => {
    setOpenThread(threadId);
    return () => setOpenThread(null);
  }, [setOpenThread, threadId]);

  if (query.isLoading) return <ReaderSkeleton />;
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
  if (!query.data) return null;
  // No placeholder data: while the next thread loads, the previous thread's
  // controller must not stay mounted, or keys would act on the wrong thread.
  // Key by thread so per-thread state (expanded, remote images) resets.
  return <ThreadReader key={query.data.thread.id} data={query.data} />;
}
