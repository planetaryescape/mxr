import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { MessageSquareReply, RefreshCw, X } from "lucide-react";
import { toast } from "sonner";

import { fetchReplyQueue, setReplyLater, type ReplyQueueMessage } from "./api";
import { MailboxList } from "@/features/mailbox/MailboxList";
import type { MessageGroupView, MessageRowView } from "@/features/mailbox/types";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";
import { formatRelativeAge } from "@/lib/utils";

const dayMonth = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });
const fullStamp = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

function toRowView(message: ReplyQueueMessage): MessageRowView {
  const date = new Date(message.date);
  const valid = !Number.isNaN(date.getTime());
  return {
    id: message.id,
    kind: "thread",
    thread_id: message.thread_id,
    provider_id: "",
    sender: message.from?.name?.trim() || message.from?.email || "unknown sender",
    sender_detail: message.from?.email,
    subject: message.subject,
    snippet: message.snippet,
    date: message.date,
    date_label: valid ? dayMonth.format(date) : "",
    date_full: valid ? fullStamp.format(date) : "",
    date_relative: valid ? formatRelativeAge(date) : "",
    unread: false,
    starred: false,
    has_attachments: false,
  };
}

export function ReplyQueueRoute() {
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

  const messages = queue.data?.messages ?? [];
  const groups: MessageGroupView[] = [
    { id: "reply-queue", label: "Reply later", rows: messages.map(toRowView) },
  ];

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
      {messages.length === 0 ? (
        <EmptyState
          icon={MessageSquareReply}
          title="No queued replies"
          description="Flag messages as reply-later to build this list."
        />
      ) : (
        <MailboxList
          groups={groups}
          mailboxPath="/reply-queue"
          rowAction={(row) => (
            <Button
              variant="ghost"
              size="sm"
              disabled={clear.isPending}
              onClick={() => clear.mutate(row.id)}
              aria-label={`Remove ${row.subject || "(no subject)"} from reply queue`}
            >
              <X className="size-3" />
              Remove
            </Button>
          )}
        />
      )}
    </div>
  );
}
