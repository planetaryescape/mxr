import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { MessageSquareReply, RefreshCw, X } from "lucide-react";
import { toast } from "sonner";

import { fetchReplyQueue, setReplyLater, type ReplyQueueMessage } from "./api";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";

export function ReplyQueueRoute() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const queue = useQuery({ queryKey: ["reply-queue"], queryFn: fetchReplyQueue });
  const clear = useMutation({
    mutationFn: (messageId: string) => setReplyLater(messageId, false),
    onSuccess: () => {
      toast.success("Removed from reply queue");
      void qc.invalidateQueries({ queryKey: ["reply-queue"] });
      void qc.invalidateQueries({ queryKey: ["mailbox"] });
    },
    onError: (error) => toast.error("Update failed", { description: error.message }),
  });

  if (queue.isLoading) {
    return <div className="p-6 text-xs text-muted-foreground">Loading reply queue...</div>;
  }
  if (queue.isError) {
    return (
      <EmptyState
        icon={RefreshCw}
        title="Reply queue unavailable"
        description={queue.error.message}
        action={<Button onClick={() => queue.refetch()}>Retry</Button>}
      />
    );
  }

  const rows = queue.data?.messages ?? [];
  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="border-b border-border px-6 py-4">
        <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          Reply later
        </div>
        <h1 className="text-xl font-semibold tracking-tight">Reply queue</h1>
        <p className="mt-1 text-2xs text-muted-foreground">
          Messages flagged for follow-up from the TUI, CLI, or web actions.
        </p>
      </header>
      {rows.length === 0 ? (
        <EmptyState
          icon={MessageSquareReply}
          title="No queued replies"
          description="Flag messages as reply-later to build this list."
        />
      ) : (
        <main className="min-h-0 flex-1 overflow-auto p-4">
          <div className="overflow-hidden rounded-xl border border-border bg-surface">
            {rows.map((message) => (
              <ReplyQueueRow
                key={message.id}
                message={message}
                clearing={clear.isPending}
                onOpen={() =>
                  navigate({
                    to: "/m/$mailbox/$threadId",
                    params: { mailbox: "inbox", threadId: message.thread_id },
                  })
                }
                onClear={() => clear.mutate(message.id)}
              />
            ))}
          </div>
        </main>
      )}
    </div>
  );
}

function ReplyQueueRow({
  message,
  clearing,
  onOpen,
  onClear,
}: {
  message: ReplyQueueMessage;
  clearing: boolean;
  onOpen: () => void;
  onClear: () => void;
}) {
  return (
    <div className="grid gap-3 border-b border-border px-4 py-3 last:border-b-0 md:grid-cols-[1fr_auto]">
      <button type="button" className="min-w-0 text-left" onClick={onOpen}>
        <div className="truncate text-sm font-medium">{message.subject || "(no subject)"}</div>
        <div className="mt-1 truncate text-2xs text-muted-foreground">
          {formatSender(message)} · {message.snippet}
        </div>
      </button>
      <Button variant="ghost" size="sm" onClick={onClear} disabled={clearing}>
        <X className="size-3" />
        Remove
      </Button>
    </div>
  );
}

function formatSender(message: ReplyQueueMessage): string {
  const name = message.from?.name?.trim();
  if (name) return name;
  return message.from?.email ?? "unknown sender";
}
