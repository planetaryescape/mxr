import { useQuery } from "@tanstack/react-query";
import { Paperclip } from "lucide-react";
import { useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { fetchThread } from "@/features/mailbox/api";
import { MessageBody } from "@/features/thread/MessageBody";
import { cn } from "@/lib/utils";

import type { Conversation as ConversationData, ConversationMessage } from "./api";
import { letterLead, paragraphBlocks, splitAsk } from "./messagesView";

/**
 * One topic as a conversation of new texts. Length decides the shape, not
 * the medium: a note of three lines or fewer reads compactly like chat
 * (theirs left, yours right, a faint tint and no bubble); anything longer
 * is a full-width letter that opens on its first paragraph, or the one
 * holding the ask. Removed text is marked, and the message as sent is one
 * key away.
 */
export function ConversationView({
  conversation,
  asSent,
  onToggleAsSent,
}: {
  conversation: ConversationData;
  /** The message shown as sent, if any. */
  asSent: string | null;
  onToggleAsSent: (messageId: string) => void;
}) {
  return (
    <ol data-testid="conversation" className="flex flex-col gap-4 px-5 py-4">
      {conversation.earlier_count > 0 ? (
        <li className="text-center text-[12px] text-muted-foreground">
          {conversation.earlier_count} earlier messages
        </li>
      ) : null}
      {conversation.messages.map((message) => (
        <MessageItem
          key={message.message_id}
          message={message}
          threadId={conversation.thread_id}
          asSent={asSent === message.message_id}
          onToggleAsSent={() => onToggleAsSent(message.message_id)}
        />
      ))}
    </ol>
  );
}

function who(message: ConversationMessage): string {
  if (message.from_me) return "You";
  return message.from.name?.split(/\s+/)[0] ?? message.from.email;
}

function when(date: string): string {
  return new Date(date).toLocaleString(undefined, {
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function MessageItem({
  message,
  threadId,
  asSent,
  onToggleAsSent,
}: {
  message: ConversationMessage;
  threadId: string;
  asSent: boolean;
  onToggleAsSent: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const compact = message.layout === "compact";
  const { lead, hidden } = letterLead(message);
  const shown = expanded || hidden === 0 ? message.text : lead;
  const mine = message.from_me;
  return (
    <li
      data-testid="conversation-message"
      data-layout={message.layout}
      data-from-me={mine ? "true" : "false"}
      className={cn("flex flex-col", compact && mine ? "items-end" : "items-start")}
    >
      <p className="mb-1 text-[12px] text-muted-foreground">
        <span className="font-medium text-foreground/90">{who(message)}</span> ·{" "}
        {when(message.date)}
      </p>
      {asSent ? (
        <AsSent threadId={threadId} messageId={message.message_id} />
      ) : (
        <div
          className={cn(
            "text-[14px] leading-6 text-foreground",
            compact
              ? cn("max-w-[34rem] rounded-md px-3 py-1.5", mine ? "bg-primary/10" : "bg-muted/60")
              : "w-full max-w-[66ch]",
          )}
        >
          {paragraphBlocks(shown).map((block) => (
            <p key={block.at} className="whitespace-pre-wrap [&+p]:mt-3">
              {splitAsk(block.text, message.ask_quote).map((part) =>
                part.ask ? (
                  <mark
                    key={part.at}
                    data-testid="ask-quote"
                    className="rounded-sm bg-primary/20 px-0.5 text-foreground"
                  >
                    {part.text}
                  </mark>
                ) : (
                  <span key={part.at}>{part.text}</span>
                ),
              )}
            </p>
          ))}
        </div>
      )}
      <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-muted-foreground">
        {!asSent && hidden > 0 ? (
          <button
            type="button"
            onClick={() => setExpanded((open) => !open)}
            className="hover:text-foreground hover:underline"
          >
            {expanded ? "Show less" : `Read all ${message.paragraphs} paragraphs`}
          </button>
        ) : null}
        {(message.attachments ?? []).map((attachment) => (
          <span key={attachment.attachment_id} className="inline-flex items-center gap-1">
            <Paperclip className="size-3" aria-hidden /> {attachment.filename} ·{" "}
            {Math.max(1, Math.ceil(attachment.size_bytes / 1024))} KB
          </span>
        ))}
        {message.trimmed_label || asSent ? (
          <button
            type="button"
            data-testid="trimmed-marker"
            onClick={onToggleAsSent}
            title="Show the message as sent (o or v)"
            className="inline-flex items-center gap-1 hover:text-foreground"
          >
            {asSent ? "back to what they wrote" : message.trimmed_label}
            <KeyChip className="h-4 px-1">v</KeyChip>
          </button>
        ) : null}
      </div>
    </li>
  );
}

/** The message as it arrived: its HTML, else its plain text. */
function AsSent({ threadId, messageId }: { threadId: string; messageId: string }) {
  const thread = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
    staleTime: 60_000,
  });
  const body = thread.data?.bodies.find((candidate) => candidate.message_id === messageId);
  if (!body) {
    return (
      <p data-testid="as-sent" className="text-[13px] text-muted-foreground">
        {thread.isError ? "Couldn't load the message as sent." : "Loading the message as sent…"}
      </p>
    );
  }
  return (
    <div data-testid="as-sent" className="w-full rounded-md border border-border">
      {body.text_html ? (
        <MessageBody html={body.text_html} />
      ) : (
        <pre className="whitespace-pre-wrap p-3 font-sans text-[13.5px] leading-6">
          {body.text_plain ?? ""}
        </pre>
      )}
    </div>
  );
}
