import { useMutation, useQuery } from "@tanstack/react-query";
import { MessageSquareReply, X } from "lucide-react";
import { toast } from "sonner";

import { fetchReplyQueue, setReplyLater, type ReplyQueueMessage } from "./api";
import { useLowTide } from "@/features/low-tide/lowTideMemory";
import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { useProjectedGroups } from "@/features/mail-actions/pendingMailOps";
import { ListWithReader } from "@/features/mailbox/ListWithReader";
import { Centered } from "@/features/mailbox/MailViewParts";
import { SweptClear } from "@/features/places/SweptClear";
import type { MessageRowView } from "@/features/mailbox/types";
import { plural } from "@/lib/format";

function toRow(message: ReplyQueueMessage): MessageRowView {
  return {
    id: message.id,
    kind: "message",
    thread_id: message.thread_id,
    provider_id: "",
    sender: message.from?.name?.trim() || message.from?.email || "Unknown sender",
    sender_detail: message.from?.email,
    subject: message.subject,
    snippet: message.snippet,
    date: message.date,
    date_label: "",
    date_full: message.date,
    date_relative: "",
    unread: false,
    starred: false,
    has_attachments: false,
  };
}

const LENS = { kind: "other" } as const;

/** Messages flagged reply-later (TUI `b`), oldest obligations first. */
export function ReplyQueueRoute() {
  const queue = useQuery({ queryKey: ["reply-queue"], queryFn: fetchReplyQueue });
  const rows = (queue.data?.messages ?? []).map(toRow);
  const groups = useProjectedGroups(
    rows.length > 0 ? [{ id: "queue", label: "Waiting on your reply", rows }] : [],
    LENS,
  );
  // Clearing the queue here (Done, archive) earns the small low tide, once.
  const lowTide = useLowTide(
    "reply_queue",
    queue.isLoading || queue.isError,
    groups.some((group) => group.rows.length > 0),
  );
  const done = useMutation({
    mutationFn: (messageId: string) => setReplyLater(messageId, false),
    onSuccess: (_, messageId) => {
      toast.success("Out of the reply queue", {
        action: {
          label: "Undo",
          onClick: () => void setReplyLater(messageId, true).then(() => invalidateMailQueries()),
        },
      });
      void invalidateMailQueries();
    },
    onError: (error) => toast.error("Couldn't update the queue", { description: error.message }),
  });

  return (
    <ListWithReader
      basePath="/reply-queue"
      title="Reply queue"
      meta={rows.length > 0 ? plural(rows.length, "message") : null}
      groups={groups}
      scopeKey="reply-queue"
      status={queue}
      rowAction={{
        label: "Done",
        icon: X,
        describe: (row) => `Done with ${row.subject || "(no subject)"}`,
        run: (row) => done.mutate(row.id),
      }}
      empty={
        lowTide ? (
          <SweptClear place="Reply queue" line="Low tide. Nobody's waiting on a reply.">
            Press b on any conversation to add it here.
          </SweptClear>
        ) : (
          <Centered
            icon={<MessageSquareReply className="size-6" />}
            title="Nothing waiting on a reply"
            body="Press b on any conversation to add it here."
          />
        )
      }
    />
  );
}
